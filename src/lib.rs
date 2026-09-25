//! Deltu core engine — foundation boundary.

pub mod event;
pub mod processing;
pub mod rules;
pub mod state;

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
