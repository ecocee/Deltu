//! Criterion benchmarks for the Deltu processing stages
//! (`context/specs/03-processing-core.md`).
//!
//! Run with `cargo bench`. Synthetic events only — no I/O, no network,
//! no clocks beyond the harness's own timing.

use criterion::{Criterion, criterion_group, criterion_main};
use deltu::{Event, Payload, PipelineConfig, ProcessingPipeline};

/// Builds a synthetic numeric event; panics on invalid arguments because a
/// benchmark with broken fixtures measures nothing.
fn numeric_event(id: &str, source: &str, kind: &str, timestamp: i64, value: f64) -> Event {
    Event::new(id, source, kind, timestamp, Payload::Numeric { value })
        .expect("benchmark fixture must be valid")
}

/// Case 1: filter + dedup pass-through — the cost of the two cheapest
/// stages for an event that survives both (non-numeric payload, so no
/// aggregation work is included).
fn bench_filter_and_dedup(c: &mut Criterion) {
    let mut pipeline =
        ProcessingPipeline::new(PipelineConfig::default()).expect("default config must be valid");
    let mut sequence: Vec<Event> = Vec::with_capacity(500);
    for i in 0..500 {
        sequence.push(
            Event::new(
                &format!("passthrough-{i}"),
                "bench/source",
                "bench.passthrough",
                1_000 + i,
                Payload::Text {
                    value: format!("value-{i}"),
                },
            )
            .expect("fixture must be valid"),
        );
    }

    c.bench_function("filter_and_dedup_passthrough", |b| {
        b.iter(|| {
            for event in &sequence {
                let event = event.clone();
                let _ = pipeline.process(event);
            }
        })
    });
}

/// Case 2: window aggregation across a boundary — one event per iteration
/// advances the window, forcing a close and an emit each time.
fn bench_window_aggregation_boundary(c: &mut Criterion) {
    let mut pipeline = ProcessingPipeline::new(PipelineConfig {
        window_ms: 1_000,
        ..PipelineConfig::default()
    })
    .expect("config must be valid");

    c.bench_function("window_aggregation_boundary", |b| {
        b.iter(|| {
            // Two events: the first fills window N, the second advances to
            // N+1 and closes N — one full close-and-emit per iteration.
            let _ = pipeline.process(numeric_event(
                "agg-fill",
                "bench/sensor",
                "bench.reading",
                10_000,
                42.0,
            ));
            let _ = pipeline.process(numeric_event(
                "agg-advance",
                "bench/sensor",
                "bench.reading",
                11_500,
                43.0,
            ));
        })
    });
}

/// Case 3: the full pipeline with aggregation — events accumulate inside a
/// single window (filter + dedup + accumulate, no close).
fn bench_full_pipeline(c: &mut Criterion) {
    let mut pipeline = ProcessingPipeline::new(PipelineConfig {
        window_ms: 1_000_000,
        ..PipelineConfig::default()
    })
    .expect("config must be valid");

    c.bench_function("full_pipeline_accumulate", |b| {
        b.iter(|| {
            // Same window index each time: filter + dedup + accumulation
            // per event, with dedup ids cycling through the cache.
            for i in 0..50 {
                let _ = pipeline.process(numeric_event(
                    &format!("full-{i}"),
                    "bench/sensor",
                    "bench.reading",
                    500_000,
                    i as f64,
                ));
            }
        })
    });
}

criterion_group!(
    benches,
    bench_filter_and_dedup,
    bench_window_aggregation_boundary,
    bench_full_pipeline
);
criterion_main!(benches);
