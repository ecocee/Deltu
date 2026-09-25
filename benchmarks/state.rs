//! Criterion benchmarks for the Deltu state store
//! (`context/specs/04-state-engine.md`).
//!
//! Run with `cargo bench`. Synthetic outputs only — no I/O, no real clock.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use deltu::{Event, Output, Payload, StateConfig, StateStore};

/// Builds a synthetic raw-event output; panics on invalid arguments because
/// a benchmark with broken fixtures measures nothing.
fn text_output(id: &str, ts: i64, value: &str) -> Output {
    Output::Event(
        Event::new(
            id,
            "bench/source",
            "bench.state",
            ts,
            Payload::Text {
                value: value.to_string(),
            },
        )
        .expect("benchmark fixture must be valid"),
    )
}

/// Case 1: sequential observe of 100 raw-event outputs — the write-path
/// cost including hashing, upsert, and recency refresh (ids rotate so the
/// store holds 100 distinct keys at steady state).
fn bench_observe_events(c: &mut Criterion) {
    let mut store = StateStore::new(StateConfig::default()).expect("default config must be valid");
    let outputs: Vec<Output> = (0..100)
        .map(|i| text_output(&format!("obs-{i}"), 1_000 + i, &format!("value-{i}")))
        .collect();

    c.bench_function("state_observe_100_events", |b| {
        b.iter(|| {
            for output in black_box(&outputs) {
                store.observe(black_box(output));
            }
        })
    });
}

/// Case 2: expire scan over a fully populated store — the periodic
/// scan-and-evict cost over 10_000 entries (the default capacity) when
/// nothing has aged out and when every entry has aged out.
fn bench_expire_scan(c: &mut Criterion) {
    let population: Vec<Output> = (0..10_000)
        .map(|i| {
            Output::Aggregated(deltu::Aggregated {
                source: format!("bench/source-{i}"),
                kind: "bench.reading".to_string(),
                window_start_ms: 0,
                window_end_ms: 1_000_000,
                count: 1,
                min: 1.0,
                max: 1.0,
                sum: 1.0,
                mean: 1.0,
                last: 1.0,
            })
        })
        .collect();

    c.bench_function("state_expire_scan_10k", |b| {
        b.iter_batched(
            || {
                // Fresh store per iteration: 10k entries at one timestamp.
                let mut store = StateStore::new(StateConfig {
                    max_entries: 10_000,
                    expire_after_ms: Some(300_000),
                })
                .expect("config must be valid");
                for output in &population {
                    store.observe(output);
                }
                store
            },
            |mut store| {
                // now = 1_300_000 → every entry is exactly at the boundary:
                // the full scan runs and every entry is removed + returned.
                let expired = store.expire(1_300_000);
                black_box(expired.len());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_observe_events, bench_expire_scan);
criterion_main!(benches);
