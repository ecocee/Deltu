//! Deltu runtime configuration: YAML or JSON file, environment overrides.
//!
//! Validated at startup; no silent defaults beyond the documented ones.

use serde::{Deserialize, Serialize};

use crate::actions::{ActionConfigError, ActionDefinition};
use crate::input::mqtt::MqttConfig;
use crate::processing::PipelineConfig;
use crate::state::StateConfig;

/// Deltu runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    /// HTTP listener settings.
    #[serde(default)]
    pub http: HttpConfig,
    /// Processing pipeline settings.
    #[serde(default)]
    pub pipeline: PipelineConfig,
    /// State store settings.
    #[serde(default)]
    pub state: StateConfig,
    /// Rule definitions.
    #[serde(default)]
    pub rules: Vec<crate::rules::Rule>,
    /// Action definitions.
    #[serde(default)]
    pub actions: Vec<ActionDefinition>,
    /// MQTT adapter settings (disabled when `enabled` is false).
    #[serde(default)]
    pub mqtt: MqttSection,
    /// Optional persistence (snapshots + historical sink). Disabled by
    /// default: the engine runs identically with an absent section.
    #[serde(default)]
    pub persistence: PersistenceSection,
    /// Bounded work-queue depth between HTTP handlers and the worker.
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: usize,
}

/// Persistence section (spec 12): strictly opt-in; absent or `enabled:
/// false` means no persistence code path runs at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistenceSection {
    /// Enable the local snapshot adapter. Default false.
    #[serde(default)]
    pub enabled: bool,
    /// Snapshot file path. Required when enabled.
    #[serde(default)]
    pub path: Option<String>,
    /// Snapshot cadence in seconds. Default 60.
    #[serde(default = "default_snapshot_interval_secs")]
    pub snapshot_interval_secs: u64,
}

fn default_snapshot_interval_secs() -> u64 {
    60
}

impl Default for PersistenceSection {
    fn default() -> Self {
        Self {
            enabled: false,
            path: None,
            snapshot_interval_secs: default_snapshot_interval_secs(),
        }
    }
}

/// MQTT section: `enabled` gates the adapter (an unconfigured MQTT
/// section must not attempt connections).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MqttSection {
    /// Start the MQTT adapter. Default false.
    #[serde(default)]
    pub enabled: bool,
    /// Adapter settings (broker, topics, QoS).
    #[serde(flatten)]
    pub mqtt: MqttConfig,
}

fn default_queue_capacity() -> usize {
    1_000
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            http: HttpConfig::default(),
            pipeline: PipelineConfig::default(),
            state: StateConfig::default(),
            rules: Vec::new(),
            actions: Vec::new(),
            mqtt: MqttSection::default(),
            persistence: PersistenceSection::default(),
            queue_capacity: default_queue_capacity(),
        }
    }
}

/// HTTP listener settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Bind address. Default `127.0.0.1:8080` (loopback: local engine).
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Maximum accepted request body in bytes. Default 1 MiB.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    /// Maximum events per batch. Default 1_000.
    #[serde(default = "default_max_batch")]
    pub max_batch: usize,
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_max_body_bytes() -> usize {
    1_048_576
}

fn default_max_batch() -> usize {
    1_000
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            max_body_bytes: default_max_body_bytes(),
            max_batch: default_max_batch(),
        }
    }
}

/// Configuration errors: one variant per invalid input, actionable
/// messages, no silent fallback.
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read.
    Read(String, std::io::Error),
    /// The file parsed as neither YAML nor JSON.
    Parse {
        /// The file path.
        path: String,
        /// The underlying parse failure.
        source: String,
    },
    /// A value failed validation.
    Validation(String),
    /// Referential integrity failed (rule → action).
    UnknownAction {
        /// Rule that referenced a missing action.
        rule_id: String,
        /// The missing action id.
        action: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(path, error) => {
                write!(f, "failed to read config file {path}: {error}")
            }
            ConfigError::Parse { path, source } => {
                write!(f, "failed to parse config file {path}: {source}")
            }
            ConfigError::Validation(message) => write!(f, "invalid configuration: {message}"),
            ConfigError::UnknownAction { rule_id, action } => {
                write!(f, "rule {rule_id} references unknown action {action:?}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl RuntimeConfig {
    /// Loads and validates configuration from a YAML or JSON file.
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| ConfigError::Read(path.to_string(), error))?;
        serde_yaml::from_str(&contents).map_err(|error| ConfigError::Parse {
            path: path.to_string(),
            source: error.to_string(),
        })
    }

    /// Cross-referential validation: rule → action references must resolve;
    /// action and rule ids must be unique. Composes the per-module
    /// validation already established (each engine validates its own
    /// config at build time).
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.queue_capacity == 0 {
            return Err(ConfigError::Validation("queue_capacity must be > 0".into()));
        }
        if self.persistence.enabled
            && self
                .persistence
                .path
                .as_deref()
                .is_none_or(|p| p.trim().is_empty())
        {
            return Err(ConfigError::Validation(
                "persistence.enabled requires a non-empty persistence.path".into(),
            ));
        }
        if self.persistence.snapshot_interval_secs == 0 {
            return Err(ConfigError::Validation(
                "persistence.snapshot_interval_secs must be > 0".into(),
            ));
        }

        // Rule + action validation happens through the engines at build
        // time; here we validate referential integrity and id uniqueness.
        let mut seen_actions = std::collections::HashSet::new();
        for action in &self.actions {
            if !seen_actions.insert(action.id.clone()) {
                return Err(ConfigError::Validation(format!(
                    "duplicate action id: {}",
                    action.id
                )));
            }
        }

        let mut seen_rules = std::collections::HashSet::new();
        for rule in &self.rules {
            if !seen_rules.insert(rule.id.clone()) {
                return Err(ConfigError::Validation(format!(
                    "duplicate rule id: {}",
                    rule.id
                )));
            }
            if let crate::rules::Trigger::Event { kind } = &rule.on
                && kind.trim().is_empty()
            {
                return Err(ConfigError::Validation(format!(
                    "rule {} has an empty trigger kind",
                    rule.id
                )));
            }
            if !self.actions.iter().any(|a| a.id == rule.action) {
                return Err(ConfigError::UnknownAction {
                    rule_id: rule.id.clone(),
                    action: rule.action.clone(),
                });
            }
        }

        Ok(())
    }

    /// Applies environment overrides: `DELTU_HTTP_BIND`, `DELTU_QUEUE_CAPACITY`.
    /// Unknown/invalid values are errors, never ignored.
    pub fn apply_env_overrides(&mut self, vars: &[(String, String)]) -> Result<(), ConfigError> {
        for (key, value) in vars {
            match key.as_str() {
                "DELTU_HTTP_BIND" => self.http.bind = value.clone(),
                "DELTU_SNAPSHOT_INTERVAL_SECS" => {
                    let parsed: u64 = value.parse().map_err(|_| {
                        ConfigError::Validation(format!(
                            "DELTU_SNAPSHOT_INTERVAL_SECS must be a positive integer, got {value:?}"
                        ))
                    })?;
                    if parsed == 0 {
                        return Err(ConfigError::Validation(
                            "DELTU_SNAPSHOT_INTERVAL_SECS must be > 0".into(),
                        ));
                    }
                    self.persistence.snapshot_interval_secs = parsed;
                }
                "DELTU_QUEUE_CAPACITY" => {
                    let parsed: usize = value.parse().map_err(|_| {
                        ConfigError::Validation(format!(
                            "DELTU_QUEUE_CAPACITY must be a positive integer, got {value:?}"
                        ))
                    })?;
                    if parsed == 0 {
                        return Err(ConfigError::Validation(
                            "DELTU_QUEUE_CAPACITY must be > 0".into(),
                        ));
                    }
                    self.queue_capacity = parsed;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Converts action definitions into the dispatcher's expected form,
    /// validating ids (delegates to the actions module's validation rules —
    /// `ActionConfigError` variants preserved through composition).
    pub fn validate_actions(&self) -> Result<Vec<ActionDefinition>, ActionConfigError> {
        let mut seen = std::collections::HashSet::new();
        for definition in &self.actions {
            if definition.id.trim().is_empty() {
                return Err(ActionConfigError::EmptyActionId(definition.id.clone()));
            }
            if !seen.insert(definition.id.clone()) {
                return Err(ActionConfigError::DuplicateActionId(definition.id.clone()));
            }
        }
        Ok(self.actions.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_disabled_by_default_and_validated_when_enabled() {
        let config = RuntimeConfig::default();
        assert!(!config.persistence.enabled);
        config.validate().unwrap(); // absent section: valid

        let enabled_no_path = RuntimeConfig {
            persistence: PersistenceSection {
                enabled: true,
                path: None,
                snapshot_interval_secs: 60,
            },
            ..RuntimeConfig::default()
        };
        assert!(enabled_no_path.validate().is_err());

        let enabled_zero_interval = RuntimeConfig {
            persistence: PersistenceSection {
                enabled: true,
                path: Some("snapshots.json".into()),
                snapshot_interval_secs: 0,
            },
            ..RuntimeConfig::default()
        };
        assert!(enabled_zero_interval.validate().is_err());

        let valid = RuntimeConfig {
            persistence: PersistenceSection {
                enabled: true,
                path: Some("snapshots.json".into()),
                snapshot_interval_secs: 30,
            },
            ..RuntimeConfig::default()
        };
        valid.validate().unwrap();
    }

    #[test]
    fn persistence_section_parses_from_yaml() {
        let yaml =
            "persistence:\n  enabled: true\n  path: /tmp/snap.json\n  snapshot_interval_secs: 15\n";
        let config: RuntimeConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.persistence.enabled);
        assert_eq!(config.persistence.path.as_deref(), Some("/tmp/snap.json"));
        assert_eq!(config.persistence.snapshot_interval_secs, 15);

        // Absent section: defaults (disabled).
        let empty: RuntimeConfig = serde_yaml::from_str("{}\n").unwrap();
        assert!(!empty.persistence.enabled);
    }
}
