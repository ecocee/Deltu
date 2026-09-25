//! The Deltu processing pipeline: filter → deduplication → aggregation →
//! change detection, composed synchronously over the event model.
//!
//! All stages are clock-free and deterministic; the pipeline assumes events
//! were validated at the adapter boundary (invariant 9) and does not
//! re-validate.

pub mod aggregate;
pub mod change;
pub mod dedup;
pub mod filter;

use crate::event::{Event, Payload};

pub use aggregate::{AggregateOutcome, Aggregated, WindowAggregator};
pub use change::{ChangeDetector, ChangeOutcome};
pub use dedup::Deduplicator;
pub use filter::KindFilter;

/// Configuration for [`ProcessingPipeline`]. Defaults are conservative for
/// edge-class devices and are a documented starting point, not performance
/// claims.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// `None` allows every kind; the default must not silently drop data.
    pub allowed_kinds: Option<Vec<String>>,
    /// Deduplicator id-cache capacity. Default 10_000.
    pub dedup_capacity: usize,
    /// Tumbling window length in event-time milliseconds. Default 60_000.
    pub window_ms: i64,
    /// Maximum open windows across all keys. Default 1_000.
    pub max_open_windows: usize,
    /// Deadband for change detection on window means. Default 0.0 (never
    /// suppress).
    pub min_delta: f64,
    /// Change-detector state capacity (keys). Default 1_000.
    pub change_state_capacity: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            allowed_kinds: None,
            dedup_capacity: 10_000,
            window_ms: 60_000,
            max_open_windows: 1_000,
            min_delta: 0.0,
            change_state_capacity: 1_000,
        }
    }
}

/// One invalid configuration value, with the offending field named.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineConfigError {
    /// Window length must be greater than zero.
    InvalidWindowLength(i64),
    /// A capacity field must be greater than zero.
    InvalidCapacity {
        /// The config field that was invalid.
        field: &'static str,
        /// The rejected value.
        value: usize,
    },
    /// `min_delta` must be finite and >= 0.0.
    InvalidMinDelta(f64),
    /// A kind allow-list entry was blank.
    EmptyKindEntry,
}

impl std::fmt::Display for PipelineConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineConfigError::InvalidWindowLength(value) => {
                write!(f, "window_ms must be greater than 0, got {value}")
            }
            PipelineConfigError::InvalidCapacity { field, value } => {
                write!(f, "{field} must be greater than 0, got {value}")
            }
            PipelineConfigError::InvalidMinDelta(value) => {
                write!(f, "min_delta must be finite and >= 0.0, got {value}")
            }
            PipelineConfigError::EmptyKindEntry => {
                write!(f, "allowed_kinds must not contain blank entries")
            }
        }
    }
}

impl std::error::Error for PipelineConfigError {}

/// What came out of the pipeline for one input event (plus any closed
/// windows it triggered).
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// A non-numeric event that survived filter + dedup; aggregation does
    /// not apply to it.
    Event(Event),
    /// A closed-window summary that passed the deadband.
    Aggregated(Aggregated),
}

/// Drop and eviction counters. Incremented only on real events; drops must
/// be observable from day one. Formal metrics arrive in Unit 10.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipelineCounters {
    /// Events rejected by the kind filter.
    pub filtered_out: u64,
    /// Events dropped as duplicate ids.
    pub duplicates: u64,
    /// Numeric events dropped as late (window already closed).
    pub late_dropped: u64,
    /// Open windows evicted by the capacity cap.
    pub windows_evicted: u64,
    /// Change-detector keys evicted by the capacity cap.
    pub change_states_evicted: u64,
    /// Window summaries suppressed by the deadband.
    pub changes_suppressed: u64,
}

/// The composed synchronous pipeline over one event stream.
///
/// One call to [`process`](ProcessingPipeline::process) yields zero, one,
/// or more outputs: a passing numeric event can close a window and emit;
/// a passing non-numeric event emits itself; drops emit nothing.
#[derive(Debug)]
pub struct ProcessingPipeline {
    config: PipelineConfig,
    filter: KindFilter,
    dedup: Deduplicator,
    aggregator: WindowAggregator,
    detector: ChangeDetector,
    counters: PipelineCounters,
}

impl ProcessingPipeline {
    /// Validates the configuration and constructs all four stages.
    pub fn new(config: PipelineConfig) -> Result<Self, PipelineConfigError> {
        if config.window_ms <= 0 {
            return Err(PipelineConfigError::InvalidWindowLength(config.window_ms));
        }
        if config.dedup_capacity == 0 {
            return Err(PipelineConfigError::InvalidCapacity {
                field: "dedup_capacity",
                value: config.dedup_capacity,
            });
        }
        if config.max_open_windows == 0 {
            return Err(PipelineConfigError::InvalidCapacity {
                field: "max_open_windows",
                value: config.max_open_windows,
            });
        }
        if !config.min_delta.is_finite() || config.min_delta < 0.0 {
            return Err(PipelineConfigError::InvalidMinDelta(config.min_delta));
        }
        if config.change_state_capacity == 0 {
            return Err(PipelineConfigError::InvalidCapacity {
                field: "change_state_capacity",
                value: config.change_state_capacity,
            });
        }

        // Constructors re-validate individually; the checks above make the
        // error surface deterministic for the composed pipeline first.
        let filter = KindFilter::new(config.allowed_kinds.clone())?;
        let dedup = Deduplicator::new(config.dedup_capacity)?;
        let aggregator = WindowAggregator::new(config.window_ms, config.max_open_windows)?;
        let detector = ChangeDetector::new(config.min_delta, config.change_state_capacity)?;

        Ok(Self {
            config,
            filter,
            dedup,
            aggregator,
            detector,
            counters: PipelineCounters::default(),
        })
    }

    /// The configuration the pipeline was built with.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Processes one event through all four stages.
    pub fn process(&mut self, event: Event) -> Vec<Output> {
        // 1. Filter: cheapest and most selective first.
        if !self.filter.allows(&event.kind) {
            self.counters.filtered_out += 1;
            return Vec::new();
        }

        // 2. Deduplication on the producer-supplied id.
        if !self.dedup.is_new(&event.id) {
            self.counters.duplicates += 1;
            return Vec::new();
        }

        // 3. Aggregation: numerics accumulate into tumbling windows;
        //    everything else passes through.
        let outcome = self.aggregator.accept(event);
        let mut outputs = Vec::new();
        let closed = match outcome {
            AggregateOutcome::Passthrough(event) => {
                // 4. Change detection only sees window summaries; raw
                //    non-numeric events bypass it entirely.
                return vec![Output::Event(event)];
            }
            AggregateOutcome::Accumulated => Vec::new(),
            AggregateOutcome::Closed(summaries) => summaries,
            AggregateOutcome::Late => {
                self.counters.late_dropped += 1;
                return Vec::new();
            }
        };

        // 4. Change detection over the closed summaries.
        for summary in closed {
            match self.detector.accept(summary) {
                ChangeOutcome::Emit(summary) => outputs.push(Output::Aggregated(summary)),
                ChangeOutcome::Suppressed => self.counters.changes_suppressed += 1,
            }
        }
        outputs
    }

    /// Counter snapshot; see [`PipelineCounters`].
    pub fn counters(&self) -> &PipelineCounters {
        &self.counters
    }

    /// Number of numeric events absorbed into currently open windows.
    pub fn open_windows(&self) -> usize {
        self.aggregator.open_windows()
    }
}

/// Builds a validated [`Event`] for tests and benchmarks.
pub fn test_event(id: &str, source: &str, kind: &str, timestamp: i64, value: f64) -> Event {
    Event::new(id, source, kind, timestamp, Payload::Numeric { value })
        .expect("test_event arguments must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline() -> ProcessingPipeline {
        ProcessingPipeline::new(PipelineConfig::default()).unwrap()
    }

    #[test]
    fn defaults_are_conservative_and_documented() {
        let config = PipelineConfig::default();
        assert_eq!(config.allowed_kinds, None);
        assert_eq!(config.dedup_capacity, 10_000);
        assert_eq!(config.window_ms, 60_000);
        assert_eq!(config.max_open_windows, 1_000);
        assert_eq!(config.min_delta, 0.0);
        assert_eq!(config.change_state_capacity, 1_000);
    }

    #[test]
    fn invalid_configs_are_rejected_with_named_fields() {
        assert_eq!(
            ProcessingPipeline::new(PipelineConfig {
                window_ms: 0,
                ..PipelineConfig::default()
            })
            .unwrap_err(),
            PipelineConfigError::InvalidWindowLength(0)
        );
        assert_eq!(
            ProcessingPipeline::new(PipelineConfig {
                dedup_capacity: 0,
                ..PipelineConfig::default()
            })
            .unwrap_err(),
            PipelineConfigError::InvalidCapacity {
                field: "dedup_capacity",
                value: 0
            }
        );
        assert_eq!(
            ProcessingPipeline::new(PipelineConfig {
                max_open_windows: 0,
                ..PipelineConfig::default()
            })
            .unwrap_err(),
            PipelineConfigError::InvalidCapacity {
                field: "max_open_windows",
                value: 0
            }
        );
        // assert_eq! cannot compare values containing NaN; assert the variant.
        assert!(matches!(
            ProcessingPipeline::new(PipelineConfig {
                min_delta: f64::NAN,
                ..PipelineConfig::default()
            })
            .unwrap_err(),
            PipelineConfigError::InvalidMinDelta(_)
        ));
        assert_eq!(
            ProcessingPipeline::new(PipelineConfig {
                change_state_capacity: 0,
                ..PipelineConfig::default()
            })
            .unwrap_err(),
            PipelineConfigError::InvalidCapacity {
                field: "change_state_capacity",
                value: 0
            }
        );
        assert_eq!(
            ProcessingPipeline::new(PipelineConfig {
                allowed_kinds: Some(vec!["  ".to_string()]),
                ..PipelineConfig::default()
            })
            .unwrap_err(),
            PipelineConfigError::EmptyKindEntry
        );
    }

    #[test]
    fn filtered_event_yields_no_output_and_counts() {
        let mut pipeline = ProcessingPipeline::new(PipelineConfig {
            allowed_kinds: Some(vec!["temperature".to_string()]),
            ..PipelineConfig::default()
        })
        .unwrap();
        let event = test_event("e1", "s", "door.state", 100, 1.0);
        assert!(pipeline.process(event).is_empty());
        assert_eq!(pipeline.counters().filtered_out, 1);
        assert_eq!(pipeline.counters().duplicates, 0);
    }

    #[test]
    fn duplicate_event_yields_no_output_and_counts() {
        let mut pipeline = pipeline();
        let first = test_event("dup", "s", "door.state", 100, 1.0);
        let second = test_event("dup", "s", "door.state", 200, 1.0);
        // Numeric events accumulate; the first yields no output, and the
        // replayed id is dropped as a duplicate.
        assert!(pipeline.process(first).is_empty());
        assert!(pipeline.process(second).is_empty());
        assert_eq!(pipeline.counters().duplicates, 1);
        assert_eq!(pipeline.counters().filtered_out, 0);
        assert_eq!(pipeline.open_windows(), 1);
    }

    #[test]
    fn numeric_sequence_crossing_boundary_emits_one_aggregate() {
        let mut pipeline = ProcessingPipeline::new(PipelineConfig {
            window_ms: 1_000,
            ..PipelineConfig::default()
        })
        .unwrap();
        // Three events in window 0, then one that advances to window 1.
        let _ = pipeline.process(test_event("a", "s", "temperature.reading", 100, 20.0));
        let _ = pipeline.process(test_event("b", "s", "temperature.reading", 200, 25.0));
        let _ = pipeline.process(test_event("c", "s", "temperature.reading", 300, 10.0));
        let outputs = pipeline.process(test_event("d", "s", "temperature.reading", 1_500, 99.0));

        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            Output::Aggregated(summary) => {
                assert_eq!(summary.source, "s");
                assert_eq!(summary.kind, "temperature.reading");
                assert_eq!(summary.window_start_ms, 0);
                assert_eq!(summary.window_end_ms, 1_000);
                assert_eq!(summary.count, 3);
                assert_eq!(summary.min, 10.0);
                assert_eq!(summary.max, 25.0);
                assert_eq!(summary.sum, 55.0);
                assert_eq!(summary.mean, 55.0 / 3.0);
                assert_eq!(summary.last, 10.0);
            }
            Output::Event(event) => panic!("expected Aggregated, got Event {event:?}"),
        }
    }

    #[test]
    fn non_numeric_sequence_passes_through_as_events() {
        let mut pipeline = pipeline();
        let e1 = Event::new(
            "n1",
            "s",
            "door.state",
            100,
            Payload::Text {
                value: "open".to_string(),
            },
        )
        .unwrap();
        let e2 = Event::new(
            "n2",
            "s",
            "door.state",
            200,
            Payload::Boolean { value: true },
        )
        .unwrap();
        let out1 = pipeline.process(e1.clone());
        let out2 = pipeline.process(e2.clone());
        assert_eq!(out1, vec![Output::Event(e1)]);
        assert_eq!(out2, vec![Output::Event(e2)]);
    }

    #[test]
    fn counter_totals_match_dropped_inputs() {
        let mut pipeline = ProcessingPipeline::new(PipelineConfig {
            allowed_kinds: Some(vec!["temperature".to_string()]),
            window_ms: 1_000,
            ..PipelineConfig::default()
        })
        .unwrap();

        // 2 filtered
        let _ = pipeline.process(test_event("f1", "s", "door.state", 100, 1.0));
        let _ = pipeline.process(test_event("f2", "s", "door.state", 200, 1.0));
        // 1 duplicate
        let _ = pipeline.process(test_event("d", "s", "temperature.reading", 100, 1.0));
        let _ = pipeline.process(test_event("d", "s", "temperature.reading", 150, 1.0));
        // 1 accepted (window 0 opens)
        let _ = pipeline.process(test_event("k", "s", "temperature.reading", 200, 1.0));
        // advances to window 1 (closes 0), then a late event for window 0
        let _ = pipeline.process(test_event("k2", "s", "temperature.reading", 1_500, 2.0));
        let _ = pipeline.process(test_event("late", "s", "temperature.reading", 500, 3.0));

        let counters = pipeline.counters();
        assert_eq!(counters.filtered_out, 2);
        assert_eq!(counters.duplicates, 1);
        assert_eq!(counters.late_dropped, 1);
        assert_eq!(counters.changes_suppressed, 0); // min_delta = 0.0
        assert_eq!(counters.windows_evicted, 0);
        assert_eq!(counters.change_states_evicted, 0);
    }
}
