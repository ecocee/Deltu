//! Deltu core engine — foundation boundary.

pub mod actions;
pub mod ai;
pub mod cli;
pub mod event;
pub mod input;
pub mod metrics;
pub mod persistence;
pub mod processing;
pub mod rules;
pub mod runtime;
pub mod state;

pub use runtime::{ConfigError, RuntimeConfig, serve};

pub use actions::{ActionConfigError, ActionError, ActionExecutor, ActionSummary, LogLevel};
pub use actions::{ActionCounters, ActionDefinition, ActionDispatcher, ActionKind, ActionOutcome};

pub use event::{Event, EventError, Payload};
pub use processing::{
    Aggregated, Output, PipelineConfig, PipelineConfigError, PipelineCounters, ProcessingPipeline,
};
pub use rules::{
    ActionRequest, Condition, EvalInput, Field, Literal, OncePerWindow, Operator, Rule,
    RuleConfigError, RuleEngine, RuleMatch, Stat, Trigger,
};
pub use state::{
    StateConfig, StateCounters, StateEntry, StateError, StateKey, StateStore, StateValue,
};

/// Returns the engine version from Cargo metadata.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::version().is_empty());
    }
}
