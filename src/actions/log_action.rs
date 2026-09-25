//! The structured log action: one JSON line per execution on stderr.
//!
//! Clock-free: the timestamp is injected by the caller (the runtime owns
//! real time; Units 03–06 stay deterministic). No logging framework yet —
//! `tracing` arrives with the runtime in Unit 07 and this action migrates
//! to it there.

use serde_json::Value as JsonValue;

/// Severity levels for log actions (closed set; serialized lowercase).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Debug severity.
    Debug,
    /// Informational severity.
    Info,
    /// Warning severity.
    Warn,
    /// Error severity.
    Error,
}

impl LogLevel {
    /// Lowercase name used in the JSON line.
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// Renders one structured log line. The rule id, payload, and caller
/// timestamp all appear; `template`, when set, becomes the `message` field
/// with `{rule_id}` and `{payload}` placeholders substituted (plain
/// replacement — no format runtime).
///
/// Only string rendering happens here; the caller decides where the line
/// goes (the built-in executor writes it to stderr).
pub fn render_line(
    rule_id: &str,
    action_id: &str,
    level: LogLevel,
    payload: &JsonValue,
    timestamp_ms: i64,
    template: Option<&str>,
) -> String {
    let message = template.map(|template| {
        template
            .replace("{rule_id}", rule_id)
            .replace("{payload}", &payload.to_string())
    });

    let line = serde_json::json!({
        "ts_ms": timestamp_ms,
        "level": level.as_str(),
        "action": action_id,
        "rule_id": rule_id,
        "payload": payload,
    });

    match message {
        Some(message) => {
            let mut line = line;
            line["message"] = JsonValue::String(message);
            line.to_string()
        }
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn line_contains_rule_id_payload_and_timestamp() {
        let line = render_line(
            "hot-room",
            "log-ops",
            LogLevel::Warn,
            &json!({ "mean": 85.0 }),
            1_769_412_000_123,
            None,
        );
        let parsed: JsonValue = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["rule_id"], "hot-room");
        assert_eq!(parsed["action"], "log-ops");
        assert_eq!(parsed["level"], "warn");
        assert_eq!(parsed["ts_ms"], 1_769_412_000_123i64);
        assert_eq!(parsed["payload"]["mean"], 85.0);
    }

    #[test]
    fn template_substitutes_placeholders() {
        let line = render_line(
            "hot-room",
            "log-ops",
            LogLevel::Error,
            &json!({ "mean": 85.0 }),
            1_000,
            Some("Rule {rule_id} fired with {payload}"),
        );
        let parsed: JsonValue = serde_json::from_str(&line).unwrap();
        assert_eq!(
            parsed["message"],
            r#"Rule hot-room fired with {"mean":85.0}"#
        );
    }

    #[test]
    fn levels_render_lowercase() {
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }
}
