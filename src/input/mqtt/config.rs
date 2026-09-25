//! MQTT adapter configuration.

use serde::{Deserialize, Serialize};

/// MQTT protocol version for the connection (resolves the tracked open
/// question: 5.0-capable client, version configured per deployment —
/// default 5.0, downgrade to 3.1.1 for older brokers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    /// MQTT 5.0 (default).
    Mqtt5,
    /// MQTT 3.1.1 for older brokers.
    Mqtt31,
}

/// MQTT adapter settings. Lives under the `mqtt:` config section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MqttConfig {
    /// Broker URL, e.g. `mqtt://broker.local:1883` (rumqttc scheme).
    pub broker_url: String,
    /// Client id; a stable id + persistent session lets the broker queue
    /// messages while Deltu restarts.
    pub client_id: String,
    /// Topic filters to subscribe (e.g. `sensors/#`).
    pub topics: Vec<String>,
    /// QoS for subscriptions and publishes. Default 1 (at least once —
    /// telemetry must not be silently lost).
    pub qos: u8,
    /// Keep-alive in seconds. Default 30.
    pub keep_alive_secs: u64,
    /// MQTT5 session expiry seconds; `None` = never expire (persistent).
    /// Ignored on the 3.1.1 transport (clean_session=false there).
    pub session_expiry_secs: Option<u32>,
    /// Protocol version. Default MQTT 5.
    pub transport: Transport,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker_url: "mqtt://127.0.0.1:1883".to_string(),
            client_id: "deltu".to_string(),
            topics: vec![],
            qos: 1,
            keep_alive_secs: 30,
            session_expiry_secs: None,
            transport: Transport::Mqtt5,
        }
    }
}
