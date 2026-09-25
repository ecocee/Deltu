//! Tumbling event-time window aggregation keyed by `(source, kind)`.

use std::collections::HashMap;

use crate::event::{Event, Payload};
use crate::processing::PipelineConfigError;

/// Summary of one closed aggregation window.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregated {
    pub source: String,
    pub kind: String,
    /// First millisecond of the window (index * window_ms).
    pub window_start_ms: i64,
    /// One past the last millisecond of the window ((index + 1) * window_ms).
    pub window_end_ms: i64,
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
    /// `sum / count`.
    pub mean: f64,
    /// Last value accumulated in the window.
    pub last: f64,
}

/// One accumulator per `(source, kind)` window.
#[derive(Debug)]
struct Window {
    index: i64,
    count: u64,
    min: f64,
    max: f64,
    sum: f64,
    last: f64,
}

impl Window {
    fn new(index: i64, value: f64) -> Self {
        Self {
            index,
            count: 1,
            min: value,
            max: value,
            sum: value,
            last: value,
        }
    }

    fn accumulate(&mut self, value: f64) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
        self.last = value;
    }

    fn close(self, source: String, kind: String, window_ms: i64) -> Aggregated {
        Aggregated {
            source,
            kind,
            window_start_ms: self.index * window_ms,
            window_end_ms: (self.index + 1) * window_ms,
            count: self.count,
            min: self.min,
            max: self.max,
            sum: self.sum,
            mean: self.sum / self.count as f64,
            last: self.last,
        }
    }
}

/// What the aggregator did with one event.
#[derive(Debug)]
pub enum AggregateOutcome {
    /// Non-numeric event: aggregation does not apply; emit unchanged.
    Passthrough(Event),
    /// Numeric event absorbed into an open (or new) window; nothing emits.
    Accumulated,
    /// The event closed one or more windows; the summaries are attached and
    /// the event itself was absorbed into the new open window.
    Closed(Vec<Aggregated>),
    /// The event arrived after its key's window had already closed; dropped.
    Late,
}

/// Tumbling event-time windows; numerics accumulate, everything else passes
/// through unchanged.
///
/// Window index = `timestamp / window_ms` (positive operands, so integer
/// truncation equals floor); a timestamp exactly on a boundary belongs to
/// the newer window. Open windows are capped: admitting a new window beyond
/// `max_open_windows` evicts the globally oldest one, which is dropped —
/// never emitted, because it never closed.
#[derive(Debug)]
pub struct WindowAggregator {
    windows: HashMap<(String, String), Window>,
    capacity: usize,
    window_ms: i64,
    last_closed_index: HashMap<(String, String), i64>,
    late_dropped: u64,
    windows_evicted: u64,
}

impl WindowAggregator {
    /// Creates an aggregator with `window_ms`-long windows, holding at most
    /// `max_open_windows` open windows across all keys.
    pub fn new(window_ms: i64, max_open_windows: usize) -> Result<Self, PipelineConfigError> {
        if window_ms <= 0 {
            return Err(PipelineConfigError::InvalidWindowLength(window_ms));
        }
        if max_open_windows == 0 {
            return Err(PipelineConfigError::InvalidCapacity {
                field: "max_open_windows",
                value: max_open_windows,
            });
        }
        Ok(Self {
            windows: HashMap::new(),
            capacity: max_open_windows,
            window_ms,
            last_closed_index: HashMap::new(),
            late_dropped: 0,
            windows_evicted: 0,
        })
    }

    /// Processes one event; see [`AggregateOutcome`].
    pub fn accept(&mut self, event: Event) -> AggregateOutcome {
        let value = match event.payload {
            Payload::Numeric { value } => value,
            _ => return AggregateOutcome::Passthrough(event),
        };

        let key = (event.source.clone(), event.kind.clone());
        let index = event.timestamp / self.window_ms;

        // Late check first: anything at or before the key's last closed
        // window is dropped.
        if let Some(&closed) = self.last_closed_index.get(&key)
            && index <= closed
        {
            self.late_dropped += 1;
            return AggregateOutcome::Late;
        }

        // Close-on-advance: a newer index closes the key's open window(s).
        let mut closed = Vec::new();
        if let Some(open) = self.windows.get(&key)
            && open.index < index
        {
            let window = self.windows.remove(&key).expect("checked presence above");
            self.last_closed_index.insert(key.clone(), window.index);
            closed.push(window.close(key.0.clone(), key.1.clone(), self.window_ms));
        }

        // Absorb into the (possibly new) open window for this key.
        match self.windows.get_mut(&key) {
            Some(open) => open.accumulate(value),
            None => {
                // Enforce the global open-window cap before opening a new
                // window. The globally oldest open window (smallest index)
                // is evicted and dropped without emitting.
                if self.windows.len() == self.capacity
                    && let Some(oldest_key) = self
                        .windows
                        .iter()
                        .min_by_key(|(_, w)| w.index)
                        .map(|(k, _)| k.clone())
                {
                    self.windows.remove(&oldest_key);
                    self.windows_evicted += 1;
                }
                self.windows.insert(key.clone(), Window::new(index, value));
            }
        }

        if closed.is_empty() {
            AggregateOutcome::Accumulated
        } else {
            AggregateOutcome::Closed(closed)
        }
    }

    /// Number of currently open windows (bounded by `max_open_windows`).
    pub fn open_windows(&self) -> usize {
        self.windows.len()
    }

    pub fn late_dropped(&self) -> u64 {
        self.late_dropped
    }

    pub fn windows_evicted(&self) -> u64 {
        self.windows_evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aggregator() -> WindowAggregator {
        WindowAggregator::new(1_000, 1_000).unwrap()
    }

    fn numeric_event(id: &str, ts: i64, value: f64) -> Event {
        Event::new(
            id,
            "sensor-1",
            "temperature.reading",
            ts,
            Payload::Numeric { value },
        )
        .unwrap()
    }

    fn text_event(id: &str, ts: i64, text: &str) -> Event {
        Event::new(
            id,
            "sensor-1",
            "door.state",
            ts,
            Payload::Text {
                value: text.to_string(),
            },
        )
        .unwrap()
    }

    #[test]
    fn non_numeric_event_passes_through_unchanged() {
        let mut agg = aggregator();
        let event = text_event("d1", 500, "open");
        match agg.accept(event.clone()) {
            AggregateOutcome::Passthrough(out) => assert_eq!(out, event),
            other => panic!("expected passthrough, got {other:?}"),
        }
    }

    #[test]
    fn accumulation_tracks_count_min_max_sum_last() {
        let mut agg = aggregator();
        assert!(matches!(
            agg.accept(numeric_event("a", 100, 20.0)),
            AggregateOutcome::Accumulated
        ));
        assert!(matches!(
            agg.accept(numeric_event("b", 200, 25.0)),
            AggregateOutcome::Accumulated
        ));
        assert!(matches!(
            agg.accept(numeric_event("c", 300, 10.0)),
            AggregateOutcome::Accumulated
        ));

        // Advance past window 0 to close it.
        assert!(matches!(
            agg.accept(numeric_event("d", 1_500, 99.0)),
            AggregateOutcome::Closed(_)
        ));

        // Reconstruct the closed summary deterministically: feed the same
        // sequence into a fresh aggregator and close it the same way.
        let mut agg2 = aggregator();
        for ts in [100, 200, 300] {
            let _ = agg2.accept(numeric_event(
                "x",
                ts,
                [20.0, 25.0, 10.0][ts as usize / 100 - 1],
            ));
        }
        match agg2.accept(numeric_event("y", 1_500, 99.0)) {
            AggregateOutcome::Closed(summaries) => {
                let s = &summaries[0];
                assert_eq!(s.count, 3);
                assert_eq!(s.min, 10.0);
                assert_eq!(s.max, 25.0);
                assert_eq!(s.sum, 55.0);
                assert_eq!(s.mean, 55.0 / 3.0);
                assert_eq!(s.last, 10.0);
                assert_eq!(s.window_start_ms, 0);
                assert_eq!(s.window_end_ms, 1_000);
            }
            other => panic!("expected closed window, got {other:?}"),
        }
    }

    #[test]
    fn close_on_advance_emits_with_correct_bounds() {
        let mut agg = aggregator();
        let _ = agg.accept(numeric_event("a", 1_234, 5.0));
        match agg.accept(numeric_event("b", 2_500, 6.0)) {
            AggregateOutcome::Closed(summaries) => {
                assert_eq!(summaries.len(), 1);
                assert_eq!(summaries[0].window_start_ms, 1_000);
                assert_eq!(summaries[0].window_end_ms, 2_000);
                assert_eq!(summaries[0].last, 5.0);
            }
            other => panic!("expected closed window, got {other:?}"),
        }
    }

    #[test]
    fn boundary_timestamp_belongs_to_newer_window() {
        let mut agg = aggregator();
        // Window 0: [0, 1000). Window 1: [1000, 2000). ts = 1000 -> window 1.
        let _ = agg.accept(numeric_event("a", 500, 1.0));
        // b's timestamp is exactly on the boundary: index = 1000/1000 = 1,
        // so it closes window 0 (which must contain only `a`) and opens
        // window 1.
        match agg.accept(numeric_event("b", 1_000, 2.0)) {
            AggregateOutcome::Closed(summaries) => {
                assert_eq!(summaries.len(), 1);
                assert_eq!(summaries[0].window_start_ms, 0);
                assert_eq!(summaries[0].window_end_ms, 1_000);
                assert_eq!(summaries[0].count, 1); // only `a` — b is in the newer window
                assert_eq!(summaries[0].last, 1.0);
            }
            other => panic!("expected closed window, got {other:?}"),
        }
        // Advancing again closes window 1, which must contain the boundary
        // event `b` (last = 2.0), proving b went to the newer window.
        match agg.accept(numeric_event("c", 3_000, 3.0)) {
            AggregateOutcome::Closed(summaries) => {
                assert_eq!(summaries[0].window_start_ms, 1_000);
                assert_eq!(summaries[0].window_end_ms, 2_000);
                assert_eq!(summaries[0].count, 1);
                assert_eq!(summaries[0].last, 2.0);
            }
            other => panic!("expected closed window, got {other:?}"),
        }
    }

    #[test]
    fn late_event_is_dropped_and_counted() {
        let mut agg = aggregator();
        let _ = agg.accept(numeric_event("a", 5_500, 1.0)); // window 5 opens
        let _ = agg.accept(numeric_event("b", 6_500, 2.0)); // closes 5, opens 6
        // ts in window 5 again: at or before the last closed index.
        assert!(matches!(
            agg.accept(numeric_event("c", 5_100, 3.0)),
            AggregateOutcome::Late
        ));
        assert_eq!(agg.late_dropped(), 1);
    }

    #[test]
    fn oldest_open_window_is_evicted_at_capacity_and_counted() {
        // Capacity 1: opening a second key's window evicts the first.
        let mut agg = WindowAggregator::new(1_000, 1).unwrap();
        let mut e1 = numeric_event("a", 100, 1.0);
        e1.source = "sensor-A".to_string();
        let mut e2 = numeric_event("b", 200, 2.0);
        e2.source = "sensor-B".to_string();
        let _ = agg.accept(e1);
        assert_eq!(agg.open_windows(), 1);
        let _ = agg.accept(e2);
        assert_eq!(agg.open_windows(), 1); // cap enforced
        assert_eq!(agg.windows_evicted(), 1);
    }

    #[test]
    fn zero_window_length_is_rejected() {
        let err = WindowAggregator::new(0, 10).unwrap_err();
        assert_eq!(err, PipelineConfigError::InvalidWindowLength(0));
    }

    #[test]
    fn zero_capacity_is_rejected() {
        let err = WindowAggregator::new(1_000, 0).unwrap_err();
        assert_eq!(
            err,
            PipelineConfigError::InvalidCapacity {
                field: "max_open_windows",
                value: 0
            }
        );
    }
}
