//! Deltu's metrics model (spec 10 / research/performance.md): a fixed-field
//! registry with bounded per-stage latency rings and a throughput window.
//!
//! Bounded-state invariant applies to metrics too: the ring overwrites its
//! oldest sample; no dynamic label explosion; no unbounded series.

use std::collections::VecDeque;
use std::time::Instant;

/// One per-stage latency ring. Capacity-bounded (default 1024): when full,
/// the oldest sample is overwritten. Exposes avg/P95/P99 on snapshot.
#[derive(Debug, Clone)]
pub struct LatencyRing {
    samples: VecDeque<u64>,
    capacity: usize,
}

impl Default for LatencyRing {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl LatencyRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
        }
    }

    /// Records one duration in milliseconds.
    pub fn record_ms(&mut self, ms: u64) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Average of retained samples in ms (0 when empty).
    pub fn avg_ms(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.samples.iter().sum();
        sum as f64 / self.samples.len() as f64
    }

    /// Nearest-rank percentile of retained samples in ms.
    pub fn percentile_ms(&self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut sorted: Vec<u64> = self.samples.iter().copied().collect();
        sorted.sort_unstable();
        let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
        let index = rank.clamp(1, sorted.len()) - 1;
        sorted[index]
    }
}

/// Sliding one-minute throughput window: event-count buckets of one second.
#[derive(Debug)]
pub struct ThroughputWindow {
    buckets: VecDeque<(Instant, u64)>,
    window_secs: u64,
    total_since_start: u64,
}

impl Default for ThroughputWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl ThroughputWindow {
    pub fn new() -> Self {
        Self::with_window_secs(60)
    }

    pub fn with_window_secs(window_secs: u64) -> Self {
        Self {
            buckets: VecDeque::new(),
            window_secs: window_secs.max(1),
            total_since_start: 0,
        }
    }

    /// Records one event arrival.
    pub fn record_event(&mut self) {
        self.record_events(1);
    }

    pub fn record_events(&mut self, count: u64) {
        let now = Instant::now();
        self.buckets.push_back((now, count));
        self.total_since_start += count;
        self.evict_before(now);
    }

    /// Events per second over the sliding window (0 when no samples).
    pub fn events_per_sec(&mut self) -> f64 {
        let now = Instant::now();
        self.evict_before(now);
        let count: u64 = self.buckets.iter().map(|(_, c)| c).sum();
        count as f64 / self.window_secs as f64
    }

    /// Total events since process start.
    pub fn total_events(&self) -> u64 {
        self.total_since_start
    }

    fn evict_before(&mut self, now: Instant) {
        let cutoff = now
            .checked_sub(std::time::Duration::from_secs(self.window_secs))
            .unwrap_or(now);
        while let Some((at, _)) = self.buckets.front() {
            if *at < cutoff {
                self.buckets.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Fixed-field registry aggregating every counter since Unit 03 plus
/// latency rings. Snapshotted by `/v1/status`.
#[derive(Debug, Default)]
pub struct MetricsRegistry {
    /// Events accepted into the pipeline (reached the pipeline boundary).
    pub events_total: ThroughputWindow,
    /// Batch handling latency (validation + enqueue receipt), ms.
    pub batch_latency: LatencyRing,
    /// Pipeline stage latency, ms.
    pub pipeline_latency: LatencyRing,
    /// Action dispatch latency, ms.
    pub action_latency: LatencyRing,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one batch: event count + batch handling duration.
    pub fn record_batch(&mut self, events: usize, elapsed: Instant) {
        self.events_total.record_events(events as u64);
        self.batch_latency
            .record_ms(elapsed.elapsed().as_millis() as u64);
    }

    /// The complete snapshot rendered into `/v1/status` (documented shape).
    pub fn snapshot(&mut self) -> serde_json::Value {
        serde_json::json!({
            "events_per_sec": self.events_total.events_per_sec(),
            "events_total": self.events_total.total_events(),
            "latency_ms": {
                "batch": {
                    "avg": self.batch_latency.avg_ms(),
                    "p95": self.batch_latency.percentile_ms(95.0),
                    "p99": self.batch_latency.percentile_ms(99.0),
                },
                "pipeline": {
                    "avg": self.pipeline_latency.avg_ms(),
                    "p95": self.pipeline_latency.percentile_ms(95.0),
                    "p99": self.pipeline_latency.percentile_ms(99.0),
                },
                "action": {
                    "avg": self.action_latency.avg_ms(),
                    "p95": self.action_latency.percentile_ms(95.0),
                    "p99": self.action_latency.percentile_ms(99.0),
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_percentile_math_on_known_inputs() {
        let mut ring = LatencyRing::new(1024);
        for ms in 1..=100 {
            ring.record_ms(ms);
        }
        assert_eq!(ring.len(), 100);
        assert!((ring.avg_ms() - 50.5).abs() < f64::EPSILON);
        assert_eq!(ring.percentile_ms(95.0), 95);
        assert_eq!(ring.percentile_ms(99.0), 99);
        assert_eq!(ring.percentile_ms(50.0), 50);
        assert_eq!(ring.percentile_ms(0.0), 1); // clamped to min
        assert_eq!(ring.percentile_ms(100.0), 100);
    }

    #[test]
    fn ring_is_bounded_and_overwrites_oldest() {
        let mut ring = LatencyRing::new(4);
        for ms in 1..=10 {
            ring.record_ms(ms);
        }
        assert_eq!(ring.len(), 4);
        // Oldest (1..=6) overwritten: retained are 7,8,9,10.
        assert_eq!(ring.percentile_ms(100.0), 10);
        assert_eq!(ring.avg_ms(), 8.5);
    }

    #[test]
    fn empty_ring_reports_zeroes() {
        let ring = LatencyRing::new(8);
        assert!(ring.is_empty());
        assert_eq!(ring.avg_ms(), 0.0);
        assert_eq!(ring.percentile_ms(99.0), 0);
    }

    #[test]
    fn throughput_window_computes_rate() {
        let mut window = ThroughputWindow::with_window_secs(60);
        for _ in 0..120 {
            window.record_event();
        }
        // 120 events inside a 60s window -> 2.0 eps.
        assert!((window.events_per_sec() - 2.0).abs() < f64::EPSILON);
        assert_eq!(window.total_events(), 120);
    }

    #[test]
    fn throughput_window_evicts_old_buckets() {
        let mut window = ThroughputWindow::with_window_secs(1);
        window.record_events(500);
        // Simulate the bucket aging out.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(window.events_per_sec(), 0.0);
        assert_eq!(window.total_events(), 500); // total never decays
    }

    #[test]
    fn snapshot_has_documented_shape() {
        let mut registry = MetricsRegistry::new();
        registry.record_batch(3, Instant::now());
        let snapshot = registry.snapshot();
        assert!(snapshot["events_per_sec"].as_f64().is_some());
        assert!(snapshot["events_total"].as_u64().is_some());
        assert!(snapshot["latency_ms"]["batch"]["p95"].as_u64().is_some());
        assert!(snapshot["latency_ms"]["pipeline"]["p99"].as_u64().is_some());
        assert!(snapshot["latency_ms"]["action"]["avg"].as_f64().is_some());
    }
}
