//! The MQTT input adapter: receives continuous device data and converts it
//! to the internal event model (research/mqtt.md; MQTT-specific behavior
//! stays inside this module).

pub mod config;
pub mod convert;

pub use config::{MqttConfig, Transport};
pub use convert::{ConversionOutcome, MqttCounters};

use rumqttc::{AsyncClient, Event as BrokerEvent, MqttOptions, Packet, QoS};
use tokio::sync::mpsc;

use crate::event::Event;

/// Running MQTT counters, shared with `/v1/status` via the runtime.
#[derive(Debug, Default)]
pub struct SharedMqttCounters(std::sync::Mutex<MqttCounters>);

impl SharedMqttCounters {
    pub fn new() -> Self {
        Self(std::sync::Mutex::new(MqttCounters::default()))
    }

    pub fn snapshot(&self) -> MqttCounters {
        self.0.lock().expect("mqtt counters poisoned").clone()
    }

    fn record(&self, update: impl FnOnce(&mut MqttCounters)) {
        let mut counters = self.0.lock().expect("mqtt counters poisoned");
        update(&mut counters);
    }
}

/// Spawns the MQTT adapter task: connects to the broker, subscribes to the
/// configured topics, and forwards validated events into the engine's
/// bounded work queue.
///
/// Reconnection is handled by the adapter (rumqttc's eventloop retries
/// immediately with no backoff): on connection errors the task sleeps with
/// capped exponential backoff (100ms → 30s) before polling again, and the
/// counter of reconnection attempts is observable via `/v1/status`. A
/// closed engine queue (engine shutting down) drops the in-flight message,
/// counts it, and ends the task — broker failures never propagate upward
/// (invariant 8 pattern).
///
/// Returns `Err` when the broker URL cannot be parsed (startup
/// configuration error, not a runtime failure).
pub fn spawn(
    config: MqttConfig,
    counters: std::sync::Arc<SharedMqttCounters>,
    engine_tx: mpsc::Sender<crate::runtime::http::WorkItem>,
    engine_capacity: usize,
) -> Result<tokio::task::JoinHandle<()>, String> {
    // QoS: u8 from config → rumqttc's closed QoS enum (validated here so
    // misconfiguration fails at startup).
    let qos = match config.qos {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        other => return Err(format!("mqtt qos must be 0, 1, or 2, got {other}")),
    };

    // Broker URL: `mqtt://host:port` (the documented scheme) — parsed here
    // so misconfiguration fails at startup with an actionable message
    // (never a runtime error). TLS (`mqtts://`) is out of scope per spec 08.
    let (host, port) = parse_broker_url(&config.broker_url)?;
    let mut options = MqttOptions::new(config.client_id.clone(), host, port);
    options.set_keep_alive(std::time::Duration::from_secs(config.keep_alive_secs));

    let (client, mut eventloop) = AsyncClient::new(options, engine_capacity);

    // Adapter-owned reconnect backoff (see the loop below).
    const RECONNECT_BASE_MS: u64 = 100;
    const RECONNECT_MAX_MS: u64 = 30_000;
    let mut backoff_ms = RECONNECT_BASE_MS;

    // Subscribe to every configured topic at the configured QoS.
    let subscriptions = config.topics.clone();

    Ok(tokio::spawn(async move {
        for topic in &subscriptions {
            if let Err(error) = client.subscribe(topic, qos).await {
                // Subscriptions are re-established by the eventloop on
                // reconnect for persistent sessions; a failed subscribe at
                // startup is counted and non-fatal.
                counters.record(|c| c.messages_rejected += 1);
                eprintln!("mqtt: failed to subscribe to {topic:?}: {error}");
            }
        }

        loop {
            match eventloop.poll().await {
                Ok(BrokerEvent::Incoming(Packet::Publish(publish))) => {
                    counters.record(|c| c.messages_received += 1);
                    let topic = publish.topic.clone();
                    let payload = publish.payload.clone();
                    let now_ms = now_unix_ms();
                    match convert::convert_message(&topic, &payload, now_ms) {
                        ConversionOutcome::Event(event) => {
                            deliver(&engine_tx, event, &counters).await;
                        }
                        ConversionOutcome::Rejected(reason) => {
                            counters.record(|c| {
                                c.messages_rejected += 1;
                                c.last_rejection = Some(reason);
                            });
                        }
                    }
                }
                Ok(BrokerEvent::Incoming(Packet::ConnAck(_))) => {
                    // Connection established: reset the backoff sequence.
                    backoff_ms = RECONNECT_BASE_MS;
                    counters.record(|c| c.reconnects += 1);
                }
                Ok(_) => {}
                Err(_) => {
                    // Connection errors: rumqttc retries immediately, so
                    // the adapter owns the backoff. Without this cap-free
                    // loop an unreachable broker burns a CPU core (audit
                    // finding: ~50k attempts/s against a refused port).
                    counters.record(|c| c.reconnects += 1);
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(RECONNECT_MAX_MS);
                }
            }
        }
    }))
}

/// Forwards one validated event into the engine queue with the bounded
/// drop-new policy (research/mqtt.md: a slow engine must not grow memory
/// without limit).
async fn deliver(
    engine_tx: &mpsc::Sender<crate::runtime::http::WorkItem>,
    event: Event,
    counters: &std::sync::Arc<SharedMqttCounters>,
) {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    match engine_tx.try_send(crate::runtime::http::WorkItem {
        events: vec![event],
        reply: reply_tx,
    }) {
        Ok(()) => {
            let _ = reply_rx.await; // engine receipt
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            counters.record(|c| c.messages_dropped += 1);
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            counters.record(|c| c.messages_dropped += 1);
        }
    }
}

/// Current wall-clock milliseconds since the Unix epoch (same injection
/// point the runtime uses for HTTP-sourced events).
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Parses `mqtt://host:port` (the documented scheme). TLS (`mqtts://`)
/// is out of scope for this unit per spec 08.
fn parse_broker_url(url: &str) -> Result<(String, u16), String> {
    let rest = url
        .strip_prefix("mqtt://")
        .ok_or_else(|| format!("broker url must start with mqtt://, got {url:?}"))?;
    let (host, port) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("broker url must include a port, got {url:?}"))?;
    if host.is_empty() {
        return Err(format!("broker url has an empty host: {url:?}"));
    }
    let port: u16 = port
        .parse()
        .map_err(|_| format!("broker url has an invalid port {port:?} in {url:?}"))?;
    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_url_parses_host_and_port() {
        assert_eq!(
            parse_broker_url("mqtt://broker.local:1883").unwrap(),
            ("broker.local".to_string(), 1883)
        );
        assert_eq!(
            parse_broker_url("mqtt://127.0.0.1:1883").unwrap(),
            ("127.0.0.1".to_string(), 1883)
        );
    }

    #[test]
    fn broker_url_errors_are_actionable() {
        assert!(
            parse_broker_url("http://broker:1883")
                .unwrap_err()
                .contains("mqtt://")
        );
        assert!(
            parse_broker_url("mqtt://broker")
                .unwrap_err()
                .contains("port")
        );
        assert!(
            parse_broker_url("mqtt://:1883")
                .unwrap_err()
                .contains("host")
        );
        assert!(
            parse_broker_url("mqtt://broker:port")
                .unwrap_err()
                .contains("invalid port")
        );
    }
}
