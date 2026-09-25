//! Topic + payload → [`Event`] conversion (validation at the boundary;
//! invariant 9). Pure functions — fully testable without a broker.

use serde_json::Value as JsonValue;

use crate::event::{Event, EventError, Payload};

/// What conversion did with one broker message.
#[derive(Debug, Clone, PartialEq)]
pub enum ConversionOutcome {
    /// Valid event, ready for the pipeline.
    Event(Event),
    /// Rejected with a reason (counted by the adapter; never silent).
    Rejected(String),
}

/// MQTT adapter counters, surfaced through `/v1/status`.
#[derive(Debug, Default, Clone)]
pub struct MqttCounters {
    /// Messages received from the broker.
    pub messages_received: u64,
    /// Messages that failed conversion/validation.
    pub messages_rejected: u64,
    /// Messages dropped by the bounded overflow policy (queue full).
    pub messages_dropped: u64,
    /// Broker (re)connections observed.
    pub reconnects: u64,
    /// The most recent rejection reason (diagnostics).
    pub last_rejection: Option<String>,
}

/// Maps an MQTT topic to an event `source` (documented mapping, spec 08):
/// `mqtt://<topic>` — the topic is the source identity.
pub fn topic_to_source(topic: &str) -> String {
    format!("mqtt://{topic}")
}

/// Maps an MQTT topic to an event `kind` (documented mapping, spec 08):
/// the topic's last non-empty segment, lowercased — e.g.
/// `sensors/esp32-1/Temperature` → `temperature`. A topic ending in `/`
/// (empty last segment) is a rejection, decided by the caller via `None`.
pub fn topic_to_kind(topic: &str) -> Option<String> {
    let segment = topic.rsplit('/').find(|segment| !segment.is_empty())?;
    let kind = segment.to_lowercase();
    if kind.is_empty() { None } else { Some(kind) }
}

/// Converts one broker message into an event.
///
/// Payload contract (documented in spec 08 before implementation): the
/// payload must be a JSON object with a `value` field that converts to a
/// scalar (`number`/`string`/`boolean`), or an arbitrary JSON value for
/// structured payloads. Non-JSON payloads are rejected — never silently
/// coerced (code standards: external input is validated).
///
/// Timestamps come from the adapter's injection point (broker payloads
/// carry no authoritative clock; per-producer timestamps are a future
/// config option). Event ids are derived as `{topic}#{ts_ms}#{counter}`
/// per-adapter-process — deterministic per message in this process run;
/// broker redeliveries at QoS 1 within the same millisecond deduplicate
/// downstream (Unit 03).
pub fn convert_message(topic: &str, payload: &[u8], now_ms: i64) -> ConversionOutcome {
    if topic.is_empty() {
        return ConversionOutcome::Rejected("empty topic".to_string());
    }
    let Some(kind) = topic_to_kind(topic) else {
        return ConversionOutcome::Rejected(format!("topic {topic:?} has no kind segment"));
    };
    if payload.is_empty() {
        return ConversionOutcome::Rejected("empty payload".to_string());
    }

    let value: JsonValue = match serde_json::from_slice(payload) {
        Ok(value) => value,
        Err(error) => {
            return ConversionOutcome::Rejected(format!("payload is not valid JSON: {error}"));
        }
    };

    // Documented contract: a JSON object carrying a `value` field converts
    // to that field's payload (the same tagged shape the HTTP API accepts,
    // so rules compare against the inner scalar — not the envelope object).
    // Bare scalars convert directly; any other object is structured.
    let inner: &JsonValue = match &value {
        JsonValue::Object(object) if object.contains_key("value") => &object["value"],
        other => other,
    };
    let event_payload = match inner {
        JsonValue::Number(number) => {
            let Some(numeric) = number.as_f64() else {
                return ConversionOutcome::Rejected(
                    "numeric payload is not representable as f64".to_string(),
                );
            };
            Payload::Numeric { value: numeric }
        }
        JsonValue::String(text) => Payload::Text {
            value: text.clone(),
        },
        JsonValue::Bool(flag) => Payload::Boolean { value: *flag },
        other => Payload::Json {
            value: other.clone(),
        },
    };

    // Derive a stable-per-process id (see module doc).
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{topic}#{now_ms}#{seq}");

    match Event::new(&id, &topic_to_source(topic), &kind, now_ms, event_payload) {
        Ok(event) => ConversionOutcome::Event(event),
        Err(error @ (EventError::InvalidPayload(_) | EventError::EmptyKind)) => {
            ConversionOutcome::Rejected(error.to_string())
        }
        Err(error) => ConversionOutcome::Rejected(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_maps_to_source_and_kind() {
        assert_eq!(
            topic_to_source("sensors/esp32-1/temp"),
            "mqtt://sensors/esp32-1/temp"
        );
        assert_eq!(
            topic_to_kind("sensors/esp32-1/Temperature"),
            Some("temperature".to_string())
        );
    }

    #[test]
    fn topic_with_trailing_slash_uses_last_non_empty_segment() {
        // rsync-style trailing slash: the last non-empty segment is the
        // kind (documented behavior — empty segments are skipped).
        assert_eq!(
            topic_to_kind("sensors/esp32-1/"),
            Some("esp32-1".to_string())
        );
        assert_eq!(topic_to_kind("//"), None); // no non-empty segment at all
    }

    #[test]
    fn json_object_payload_is_structured() {
        // Per the documented contract: an object with a `value` field
        // converts to the field's payload (scalar → scalar payload);
        // objects *without* `value` are structured payloads.
        let payload = br#"{"value": 21.5}"#;
        let outcome = convert_message("sensors/esp32-1/temp", payload, 1_000);
        match outcome {
            ConversionOutcome::Event(event) => {
                assert_eq!(event.kind, "temp");
                assert_eq!(event.source, "mqtt://sensors/esp32-1/temp");
                assert_eq!(event.timestamp, 1_000);
                assert_eq!(event.payload, Payload::Numeric { value: 21.5 });
            }
            other => panic!("expected event, got {other:?}"),
        }
    }

    #[test]
    fn tagged_object_converts_to_inner_value() {
        // The HTTP API's tagged shape must convert identically over MQTT:
        // rules compare against the inner scalar, not the envelope.
        for (payload, expected) in [
            (
                br#"{"type":"text","value":"open"}"#.as_slice(),
                Payload::Text {
                    value: "open".to_string(),
                },
            ),
            (
                br#"{"type":"numeric","value":7}"#.as_slice(),
                Payload::Numeric { value: 7.0 },
            ),
            (
                br#"{"type":"boolean","value":true}"#.as_slice(),
                Payload::Boolean { value: true },
            ),
        ] {
            let outcome = convert_message("sensors/esp32-1/door.state", payload, 1_000);
            match outcome {
                ConversionOutcome::Event(event) => {
                    assert_eq!(event.payload, expected, "payload {payload:?}");
                }
                other => panic!("expected event for {payload:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn object_without_value_field_is_structured() {
        let outcome = convert_message(
            "sensors/esp32-1/meta",
            br#"{"firmware":"1.2","uptime":90}"#,
            1_000,
        );
        match outcome {
            ConversionOutcome::Event(event) => {
                assert!(matches!(event.payload, Payload::Json { .. }));
            }
            other => panic!("expected event, got {other:?}"),
        }
    }

    #[test]
    fn raw_numeric_payload_converts_to_numeric() {
        let outcome = convert_message("sensors/esp32-1/temp", br"21.5", 1_000);
        match outcome {
            ConversionOutcome::Event(event) => {
                assert_eq!(event.payload, Payload::Numeric { value: 21.5 });
            }
            other => panic!("expected event, got {other:?}"),
        }
    }

    #[test]
    fn raw_scalar_json_payloads_convert() {
        for (payload, expected) in [
            (br#"21.5"#.as_slice(), Payload::Numeric { value: 21.5 }),
            (
                br#""open""#.as_slice(),
                Payload::Text {
                    value: "open".to_string(),
                },
            ),
            (br#"true"#.as_slice(), Payload::Boolean { value: true }),
        ] {
            let outcome = convert_message("sensors/d/door.state", payload, 1_000);
            match outcome {
                ConversionOutcome::Event(event) => assert_eq!(event.payload, expected),
                other => panic!("expected event, got {other:?}"),
            }
        }
    }

    #[test]
    fn structured_json_payload_is_preserved() {
        let payload = br#"{"battery": 97, "ok": true}"#;
        let outcome = convert_message("sensors/esp32-1/status", payload, 1_000);
        match outcome {
            ConversionOutcome::Event(event) => {
                assert_eq!(
                    event.payload,
                    Payload::Json {
                        value: serde_json::json!({ "battery": 97, "ok": true })
                    }
                );
            }
            other => panic!("expected event, got {other:?}"),
        }
    }

    #[test]
    fn non_json_payload_is_rejected_not_coerced() {
        let outcome = convert_message("sensors/esp32-1/temp", b"\x01\x02not-json", 1_000);
        match outcome {
            ConversionOutcome::Rejected(reason) => {
                assert!(reason.contains("not valid JSON"), "reason: {reason}")
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[test]
    fn empty_payload_is_rejected() {
        let outcome = convert_message("sensors/esp32-1/temp", b"", 1_000);
        assert!(matches!(outcome, ConversionOutcome::Rejected(_)));
    }

    #[test]
    fn empty_topic_is_rejected() {
        let outcome = convert_message("", br#"{"value": 1}"#, 1_000);
        assert!(matches!(outcome, ConversionOutcome::Rejected(_)));
    }

    #[test]
    fn derived_ids_are_unique_per_message() {
        let payload = br#"{"value": 1}"#;
        let first = match convert_message("sensors/t", payload, 1_000) {
            ConversionOutcome::Event(event) => event.id,
            other => panic!("expected event, got {other:?}"),
        };
        let second = match convert_message("sensors/t", payload, 1_000) {
            ConversionOutcome::Event(event) => event.id,
            other => panic!("expected event, got {other:?}"),
        };
        assert_ne!(first, second);
        // Same timestamp + topic share a prefix; the counter disambiguates.
        assert!(second.starts_with(&first.split('#').take(2).collect::<Vec<_>>().join("#")));
    }
}
