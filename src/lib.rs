//! Deltu core engine — foundation boundary.

pub mod event;
pub mod processing;

pub use event::{Event, EventError, Payload};
pub use processing::{
    Aggregated, Output, PipelineConfig, PipelineConfigError, PipelineCounters, ProcessingPipeline,
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
