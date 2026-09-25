//! Payload types for Deltu events.

use serde::{Deserialize, Serialize};

/// The value carried by an event.
///
/// A closed set of payload kinds keeps common telemetry strongly typed and
/// rule-comparable; [`Payload::Json`] is the escape hatch for structured
/// data. Arbitrary untyped payloads are not accepted.
///
/// Variants are struct variants (not newtype variants) because serde's
/// internally tagged representation cannot serialize newtype variants of
/// primitive types; see `context/specs/02-event-model.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Payload {
    /// A measurement or counter. NaN and infinities are rejected at
    /// validation time because they break deterministic rule comparison.
    Numeric {
        /// The measured value.
        value: f64,
    },
    /// A short textual value. Stored trimmed; empty after trim is rejected.
    Text {
        /// The textual value.
        value: String,
    },
    /// A binary state flag.
    Boolean {
        /// The flag value.
        value: bool,
    },
    /// Structured payload for anything outside the closed kinds above.
    /// Valid JSON by construction.
    Json {
        /// The structured value.
        value: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_payload_tagged_json_shape() {
        let json = serde_json::to_value(Payload::Numeric { value: 21.5 }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "numeric", "value": 21.5 })
        );
    }

    #[test]
    fn json_payload_round_trips_arbitrary_structure() {
        let value = serde_json::json!({ "list": [1, 2, 3], "ok": true });
        let json = serde_json::to_value(Payload::Json {
            value: value.clone(),
        })
        .unwrap();
        assert_eq!(json["type"], "json");
        let back: Payload = serde_json::from_value(json).unwrap();
        assert_eq!(back, Payload::Json { value });
    }

    #[test]
    fn unknown_payload_type_is_rejected() {
        let raw = serde_json::json!({ "type": "mystery", "value": 1 });
        assert!(serde_json::from_value::<Payload>(raw).is_err());
    }
}
