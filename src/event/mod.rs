//! The Deltu internal event model.
//!
//! Every input adapter converts external data into [`Event`]; every
//! processing stage consumes it. External input is validated before
//! entering the pipeline (architecture invariant 9).

pub mod error;
pub mod payload;

pub use error::EventError;
pub use payload::Payload;

use serde::{Deserialize, Serialize};

/// A single normalized event in the Deltu processing pipeline.
///
/// Construct via [`Event::new`] (validates and normalizes) or by
/// deserializing external input followed by [`Event::validate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Producer-supplied identity; the deduplication key in Unit 03.
    pub id: String,
    /// Non-empty origin, e.g. `mqtt://broker.local/sensors/esp32-1`.
    pub source: String,
    /// Dotted event type, normalized to lowercase,
    /// e.g. `temperature.reading`.
    pub kind: String,
    /// Unix epoch milliseconds UTC; must be positive.
    pub timestamp: i64,
    /// The measured or reported value.
    pub payload: Payload,
}

impl Event {
    /// Validates inputs, normalizes fields, and returns a new event.
    ///
    /// Normalization: `id`, `source`, and `kind` are trimmed; `kind` is
    /// lowercased. `id` and `source` keep their case and format.
    pub fn new(
        id: &str,
        source: &str,
        kind: &str,
        timestamp: i64,
        payload: Payload,
    ) -> Result<Self, EventError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(EventError::EmptyId);
        }
        let source = source.trim();
        if source.is_empty() {
            return Err(EventError::EmptySource);
        }
        let kind = kind.trim();
        if kind.is_empty() {
            return Err(EventError::EmptyKind);
        }
        if timestamp <= 0 {
            return Err(EventError::InvalidTimestamp(timestamp));
        }
        validate_payload(&payload)?;

        Ok(Self {
            id: id.to_string(),
            source: source.to_string(),
            kind: kind.to_lowercase(),
            timestamp,
            payload,
        })
    }

    /// Validates an event in place; the adapter path
    /// (deserialize → validate → pipeline) uses this.
    ///
    /// Returns the event unchanged on success.
    pub fn validate(self) -> Result<Self, EventError> {
        if self.id.trim().is_empty() {
            return Err(EventError::EmptyId);
        }
        if self.source.trim().is_empty() {
            return Err(EventError::EmptySource);
        }
        if self.kind.trim().is_empty() {
            return Err(EventError::EmptyKind);
        }
        if self.timestamp <= 0 {
            return Err(EventError::InvalidTimestamp(self.timestamp));
        }
        validate_payload(&self.payload)?;
        Ok(self)
    }
}

/// Shared payload rules: finite numerics, non-empty trimmed text.
fn validate_payload(payload: &Payload) -> Result<(), EventError> {
    match payload {
        Payload::Numeric { value } => {
            if value.is_finite() {
                Ok(())
            } else {
                Err(EventError::InvalidPayload(
                    "numeric payload must be finite (NaN and infinities are rejected)",
                ))
            }
        }
        Payload::Text { value } => {
            if value.trim().is_empty() {
                Err(EventError::InvalidPayload(
                    "text payload must not be empty or whitespace-only",
                ))
            } else {
                Ok(())
            }
        }
        Payload::Boolean { value: _ } | Payload::Json { value: _ } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_event() -> Event {
        Event::new(
            "evt-000001",
            "mqtt://broker.local/sensors/esp32-1",
            "Temperature.Reading",
            1_769_412_000_123,
            Payload::Numeric { value: 21.5 },
        )
        .unwrap()
    }

    // --- valid construction for every payload variant ---

    #[test]
    fn constructs_with_numeric_payload() {
        let event = valid_event();
        assert_eq!(event.kind, "temperature.reading");
        assert_eq!(event.payload, Payload::Numeric { value: 21.5 });
    }

    #[test]
    fn constructs_with_text_payload() {
        let event = Event::new(
            "id",
            "src",
            "door.state",
            1,
            Payload::Text {
                value: "open".into(),
            },
        )
        .unwrap();
        assert_eq!(
            event.payload,
            Payload::Text {
                value: "open".into()
            }
        );
    }

    #[test]
    fn constructs_with_boolean_payload() {
        let event = Event::new(
            "id",
            "src",
            "door.state",
            1,
            Payload::Boolean { value: true },
        )
        .unwrap();
        assert_eq!(event.payload, Payload::Boolean { value: true });
    }

    #[test]
    fn constructs_with_json_payload() {
        let value = serde_json::json!({ "battery": 97 });
        let event = Event::new(
            "id",
            "src",
            "device.status",
            1,
            Payload::Json {
                value: value.clone(),
            },
        )
        .unwrap();
        assert_eq!(event.payload, Payload::Json { value });
    }

    // --- each validation rule rejected with the expected variant ---

    #[test]
    fn rejects_empty_id() {
        let err =
            Event::new("   ", "src", "kind", 1, Payload::Boolean { value: true }).unwrap_err();
        assert_eq!(err, EventError::EmptyId);
    }

    #[test]
    fn rejects_empty_source() {
        let err = Event::new("id", "", "kind", 1, Payload::Boolean { value: true }).unwrap_err();
        assert_eq!(err, EventError::EmptySource);
    }

    #[test]
    fn rejects_empty_kind() {
        let err = Event::new("id", "src", "  ", 1, Payload::Boolean { value: true }).unwrap_err();
        assert_eq!(err, EventError::EmptyKind);
    }

    #[test]
    fn rejects_zero_timestamp() {
        let err = Event::new("id", "src", "kind", 0, Payload::Boolean { value: true }).unwrap_err();
        assert_eq!(err, EventError::InvalidTimestamp(0));
    }

    #[test]
    fn rejects_negative_timestamp() {
        let err =
            Event::new("id", "src", "kind", -5, Payload::Boolean { value: true }).unwrap_err();
        assert_eq!(err, EventError::InvalidTimestamp(-5));
    }

    #[test]
    fn rejects_nan_payload() {
        let err =
            Event::new("id", "src", "kind", 1, Payload::Numeric { value: f64::NAN }).unwrap_err();
        assert!(matches!(err, EventError::InvalidPayload(_)));
    }

    #[test]
    fn rejects_positive_infinity_payload() {
        let err = Event::new(
            "id",
            "src",
            "kind",
            1,
            Payload::Numeric {
                value: f64::INFINITY,
            },
        )
        .unwrap_err();
        assert!(matches!(err, EventError::InvalidPayload(_)));
    }

    #[test]
    fn rejects_negative_infinity_payload() {
        let err = Event::new(
            "id",
            "src",
            "kind",
            1,
            Payload::Numeric {
                value: f64::NEG_INFINITY,
            },
        )
        .unwrap_err();
        assert!(matches!(err, EventError::InvalidPayload(_)));
    }

    #[test]
    fn rejects_blank_text_payload() {
        let err = Event::new(
            "id",
            "src",
            "kind",
            1,
            Payload::Text {
                value: "   ".into(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, EventError::InvalidPayload(_)));
    }

    // --- normalization ---

    #[test]
    fn trims_and_lowercases_kind_preserving_id_and_source_case() {
        let event = Event::new(
            "  Event-42  ",
            "  MQTT://Broker/Sensors  ",
            "  Temperature.Reading  ",
            1,
            Payload::Numeric { value: 1.0 },
        )
        .unwrap();
        assert_eq!(event.id, "Event-42"); // case preserved
        assert_eq!(event.source, "MQTT://Broker/Sensors"); // case preserved
        assert_eq!(event.kind, "temperature.reading"); // trimmed + lowercased
    }

    // --- JSON round-trip for all payload variants + documented shape ---

    #[test]
    fn numeric_event_json_round_trip_and_documented_shape() {
        let event = valid_event();
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "evt-000001",
                "source": "mqtt://broker.local/sensors/esp32-1",
                "kind": "temperature.reading",
                "timestamp": 1_769_412_000_123i64,
                "payload": { "type": "numeric", "value": 21.5 }
            })
        );
        let back: Event = serde_json::from_value(json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn text_event_json_round_trip() {
        let event = Event::new(
            "id",
            "src",
            "door.state",
            1,
            Payload::Text {
                value: "open".into(),
            },
        )
        .unwrap();
        let back: Event = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn boolean_event_json_round_trip() {
        let event = Event::new(
            "id",
            "src",
            "door.state",
            1,
            Payload::Boolean { value: false },
        )
        .unwrap();
        let back: Event = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn json_event_json_round_trip() {
        let event = Event::new(
            "id",
            "src",
            "device.status",
            1,
            Payload::Json {
                value: serde_json::json!({ "battery": 97, "ok": true }),
            },
        )
        .unwrap();
        let back: Event = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(back, event);
    }

    // --- external-input path: deserialize -> validate -> pipeline ---

    #[test]
    fn deserialized_valid_event_passes_validate() {
        let raw = serde_json::json!({
            "id": "evt-9",
            "source": "http://app.local/feed",
            "kind": "order.created",
            "timestamp": 1_769_412_000_999i64,
            "payload": { "type": "numeric", "value": 3.25 }
        });
        let event: Event = serde_json::from_value(raw).unwrap();
        assert!(event.validate().is_ok());
    }

    #[test]
    fn deserialized_negative_timestamp_fails_validate() {
        let raw = serde_json::json!({
            "id": "evt-9",
            "source": "http://app.local/feed",
            "kind": "order.created",
            "timestamp": -1_000,
            "payload": { "type": "numeric", "value": 3.25 }
        });
        let event: Event = serde_json::from_value(raw).unwrap();
        assert_eq!(
            event.validate().unwrap_err(),
            EventError::InvalidTimestamp(-1_000)
        );
    }
}
