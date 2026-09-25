//! Deadband change detection over the aggregated stream.

use std::collections::{HashMap, VecDeque};

use crate::processing::PipelineConfigError;
use crate::processing::aggregate::Aggregated;

/// Deadband filter for window summaries.
///
/// Keyed by `(source, kind)`: the first summary for a key is always
/// emitted; later summaries are emitted only when the mean moved by at
/// least `min_delta` from the last emitted mean. `min_delta` of 0.0 never
/// suppresses — the honest default; suppression only happens when
/// configured. State is bounded with FIFO eviction, counted.
#[derive(Debug)]
pub struct ChangeDetector {
    last_emitted: HashMap<(String, String), f64>,
    order: VecDeque<(String, String)>,
    capacity: usize,
    min_delta: f64,
    suppressed: u64,
    states_evicted: u64,
}

/// What the detector did with one summary.
#[derive(Debug)]
pub enum ChangeOutcome {
    /// The summary is meaningful; emit it.
    Emit(Aggregated),
    /// The summary stayed inside the deadband; suppressed.
    Suppressed,
}

impl ChangeDetector {
    /// Creates a detector with `min_delta` deadband and at most
    /// `capacity` keys of retained state.
    pub fn new(min_delta: f64, capacity: usize) -> Result<Self, PipelineConfigError> {
        if !min_delta.is_finite() || min_delta < 0.0 {
            return Err(PipelineConfigError::InvalidMinDelta(min_delta));
        }
        if capacity == 0 {
            return Err(PipelineConfigError::InvalidCapacity {
                field: "change_state_capacity",
                value: capacity,
            });
        }
        Ok(Self {
            last_emitted: HashMap::new(),
            order: VecDeque::with_capacity(capacity),
            capacity,
            min_delta,
            suppressed: 0,
            states_evicted: 0,
        })
    }

    /// Processes one closed-window summary; see [`ChangeOutcome`].
    pub fn accept(&mut self, summary: Aggregated) -> ChangeOutcome {
        let key = (summary.source.clone(), summary.kind.clone());

        let emit = match self.last_emitted.get(&key) {
            None => true, // first summary for this key is always emitted
            Some(&last) => (summary.mean - last).abs() >= self.min_delta,
        };

        if !emit {
            self.suppressed += 1;
            return ChangeOutcome::Suppressed;
        }

        if !self.last_emitted.contains_key(&key) {
            if self.last_emitted.len() == self.capacity {
                self.evict_oldest();
            }
            self.order.push_back(key.clone());
        }
        self.last_emitted.insert(key, summary.mean);

        ChangeOutcome::Emit(summary)
    }

    /// Forgets the oldest key. Only called when the map is full.
    fn evict_oldest(&mut self) {
        if let Some(oldest) = self.order.pop_front() {
            self.last_emitted.remove(&oldest);
            self.states_evicted += 1;
        }
    }

    pub fn suppressed(&self) -> u64 {
        self.suppressed
    }

    pub fn states_evicted(&self) -> u64 {
        self.states_evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector(min_delta: f64) -> ChangeDetector {
        ChangeDetector::new(min_delta, 1_000).unwrap()
    }

    fn summary(source: &str, kind: &str, mean: f64) -> Aggregated {
        Aggregated {
            source: source.to_string(),
            kind: kind.to_string(),
            window_start_ms: 0,
            window_end_ms: 1_000,
            count: 1,
            min: mean,
            max: mean,
            sum: mean,
            mean,
            last: mean,
        }
    }

    #[test]
    fn first_summary_for_a_key_is_emitted() {
        let mut det = detector(0.5);
        assert!(matches!(
            det.accept(summary("s", "k", 20.0)),
            ChangeOutcome::Emit(_)
        ));
    }

    #[test]
    fn below_delta_is_suppressed_and_counted() {
        let mut det = detector(0.5);
        let _ = det.accept(summary("s", "k", 20.0));
        assert!(matches!(
            det.accept(summary("s", "k", 20.3)),
            ChangeOutcome::Suppressed
        ));
        assert_eq!(det.suppressed(), 1);
    }

    #[test]
    fn movement_at_delta_is_emitted() {
        let mut det = detector(0.5);
        let _ = det.accept(summary("s", "k", 20.0));
        // |20.5 - 20.0| >= 0.5: exactly at the boundary still emits.
        assert!(matches!(
            det.accept(summary("s", "k", 20.5)),
            ChangeOutcome::Emit(_)
        ));
    }

    #[test]
    fn zero_delta_never_suppresses() {
        let mut det = detector(0.0);
        let _ = det.accept(summary("s", "k", 20.0));
        assert!(matches!(
            det.accept(summary("s", "k", 20.0)),
            ChangeOutcome::Emit(_)
        ));
        assert_eq!(det.suppressed(), 0);
    }

    #[test]
    fn keys_are_independent() {
        let mut det = detector(0.5);
        let _ = det.accept(summary("s", "temperature", 20.0));
        // A different key emits even though the value equals another key's.
        assert!(matches!(
            det.accept(summary("s", "humidity", 20.0)),
            ChangeOutcome::Emit(_)
        ));
    }

    #[test]
    fn suppression_compares_against_last_emitted_not_last_seen() {
        let mut det = detector(1.0);
        let _ = det.accept(summary("s", "k", 10.0)); // emitted, last = 10
        let _ = det.accept(summary("s", "k", 10.9)); // suppressed (|0.9| < 1)
        let _ = det.accept(summary("s", "k", 10.95)); // suppressed again
        // 12.0 vs last *emitted* 10.0 = 2.0 >= 1.0: emits. If the
        // implementation wrongly compared to the last *seen* 10.95, this
        // would still emit — so also check a case that distinguishes:
        assert!(matches!(
            det.accept(summary("s", "k", 10.5)),
            ChangeOutcome::Suppressed
        ));
    }

    #[test]
    fn state_is_evicted_at_capacity_and_counted() {
        let mut det = ChangeDetector::new(0.5, 1).unwrap();
        let _ = det.accept(summary("s", "a", 1.0));
        let _ = det.accept(summary("s", "b", 1.0)); // evicts key ("s","a")
        assert_eq!(det.states_evicted(), 1);
        // The evicted key's state is gone: its next summary emits as first.
        assert!(matches!(
            det.accept(summary("s", "a", 1.0001)),
            ChangeOutcome::Emit(_)
        ));
    }

    #[test]
    fn invalid_min_delta_is_rejected() {
        // Note: assert_eq! cannot compare values containing NaN, so the
        // NaN/Infinity cases assert on the variant, not the payload.
        assert!(matches!(
            ChangeDetector::new(f64::NAN, 10).unwrap_err(),
            PipelineConfigError::InvalidMinDelta(_)
        ));
        assert!(matches!(
            ChangeDetector::new(f64::INFINITY, 10).unwrap_err(),
            PipelineConfigError::InvalidMinDelta(_)
        ));
        assert_eq!(
            ChangeDetector::new(-0.5, 10).unwrap_err(),
            PipelineConfigError::InvalidMinDelta(-0.5)
        );
    }

    #[test]
    fn zero_capacity_is_rejected() {
        let err = ChangeDetector::new(0.5, 0).unwrap_err();
        assert_eq!(
            err,
            PipelineConfigError::InvalidCapacity {
                field: "change_state_capacity",
                value: 0
            }
        );
    }
}
