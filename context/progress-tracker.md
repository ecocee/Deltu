# Progress Tracker

Update this file after every meaningful implementation change.

## Current Phase

- Unit 04 — State Engine: **IN PROGRESS** (spec written, ready to implement)

## Current Goal

- Implement `context/specs/04-state-engine.md`: the keyed, bounded,
  expiring current-state store (`StateStore`) with its observe write path,
  narrow read interface, periodic expiration, and criterion benchmark —
  then verify the complete checklist.

## Completed

- Unit 03 — Processing Core: **COMPLETE** (2026-09-25, per
  `context/specs/03-processing-core.md`, branch `feat/03-processing-core`)
  - Four synchronous, clock-free stages: `KindFilter` (exact-kind or
    dotted-family matching, `None` = allow-all), `Deduplicator` (bounded id
    cache, FIFO eviction), `WindowAggregator` (tumbling event-time windows
    keyed by `(source, kind)`, close-on-advance, late events dropped and
    counted, `max_open_windows` eviction, non-numeric passthrough),
    `ChangeDetector` (deadband on window means, `min_delta` 0.0 = never
    suppress, bounded FIFO-evicted state).
  - `ProcessingPipeline` composition (`process: Event -> Vec<Output>`),
    `PipelineConfig` with conservative defaults, `PipelineConfigError`,
    `PipelineCounters` covering every drop/eviction class; crate-root
    re-exports.
  - Dependency added: `criterion` 0.8.2 as dev-dependency (spec-justified);
    bench target registered explicitly (`benchmarks/processing.rs`,
    `harness = false`) because Cargo autodiscovers only `benches/`.
  - Verification results: `cargo check` clean; `cargo build` ok;
    `cargo test` 61 passed / 0 failed (foundation + event-model tests still
    green); `cargo run` → `deltu 0.1.0`; `cargo fmt --check` clean;
    `cargo clippy --all-targets` 0 warnings; `cargo bench` ran all three
    cases.
  - Benchmark results (criterion 0.8.2, Apple M5, idle dev machine, bench
    profile, 100 samples):
    - `filter_and_dedup_passthrough` (500-event batch): 36.9 µs/batch
      (~74 ns/event).
    - `window_aggregation_boundary` (close + emit per iteration, 2 events):
      126 ns/iteration (~63 ns/event).
    - `full_pipeline_accumulate` (50-event batch, single window): 4.34 µs/
      batch (~87 ns/event).
    Numbers are the documented starting point for this hardware only; no
    other performance claims are made.
- Unit 00 — Planning, Research & Context: **COMPLETE** (2026-09-25)
  - Context knowledge base, research collection, decision records 001–004,
    and specs 00/01 reviewed and in use by the implemented Unit 01.
- Unit 02 — Event Model: **COMPLETE** (2026-09-25, per
  `context/specs/02-event-model.md`, branch `feat/02-event-model`)
  - `event` module (`mod.rs`, `payload.rs`, `error.rs`): `Event`, closed
    `Payload` enum, `EventError` with actionable Display messages;
    re-exported at the crate root.
  - Validation/normalization per spec: trimmed non-empty id/source/kind,
    lowercased kind, positive epoch-millis timestamp, finite numerics,
    non-empty text; `Event::new` and the adapter path
    (deserialize → `validate` → pipeline) both covered.
  - Dependencies added: `serde` 1.0.229 (derive) + `serde_json` 1.0.151,
    the two justified in the spec; pinned in `Cargo.lock`.
  - Spec refined during implementation: `Payload` uses **struct** variants
    (`Numeric { value: f64 }`) because serde's internally tagged
    representation cannot serialize primitive newtype variants; the
    documented JSON contract is unchanged.
  - Verification results: `cargo check` clean; `cargo build` ok;
    `cargo test` 24 passed / 0 failed (foundation test still green);
    `cargo run` → `deltu 0.1.0`; `cargo fmt --check` clean;
    `cargo clippy` 0 warnings. All spec checklist items pass.
- Unit 01 — Rust Foundation: **COMPLETE** (2026-09-25, per
  `context/specs/01-rust-foundation.md`)
  - `cargo init --name deltu` at repository root; edition 2024; zero
    dependencies in `Cargo.toml`.
  - `src/lib.rs` minimal core boundary (`deltu::version()`);
    `src/main.rs` prints `deltu 0.1.0` via the library.
  - Error/result conventions established in the spec (local error enums,
    `From` conversions, no `unwrap()` in production paths) for reuse by
    later units.
  - Verification results: `cargo check` clean (0 warnings); `cargo build`
    ok; `cargo test` 1 passed / 0 failed; `cargo run` → `deltu 0.1.0`;
    `cargo fmt --check` clean; `cargo clippy` 0 warnings. All spec checklist
    items pass.

## In Progress

- Unit 04 — State Engine (started 2026-09-25)
  - Spec complete: `context/specs/04-state-engine.md` — `StateKey` =
    `(source, kind)`, `StateValue` mirror of `Payload` (no serde),
    observe-based write path from pipeline outputs, sorted deterministic
    reads, injected-clock periodic expiration returning evicted entries,
    LRU-by-last-update capacity bounding, conservative defaults
    (10k entries / 5-min expiry), zero new dependencies.
  - Implementation not started.

## Next Up

- After Unit 04: write `context/specs/05-rules.md` (deterministic typed
  condition evaluation over events and state per `research/rules.md`).

## Open Questions

- CLI binary name: architecture and build-plan examples use `plan-c` while the
  project is named Deltu. The final binary name must be confirmed before the CLI
  unit (Unit 09). Does not block earlier units.
- MQTT protocol version default for the first MQTT unit: 3.1.1 vs 5.0 (research
  suggests 5.0-capable client with a configured default; decide in that unit's spec).

## Architecture Decisions

- 001 — Rust core engine, single self-hosted binary (`context/decisions/001-rust-core.md`).
- 002 — Memory-first runtime state; persistence stays optional
  (`context/decisions/002-memory-first-state.md`).
- 003 — AI is optional and isolated behind a provider abstraction
  (`context/decisions/003-ai-optional.md`).
- 004 — Async runtime deferred until continuous processing requires it
  (`context/decisions/004-deferred-async-runtime.md`).

## Session Notes

- Repository layout: `Cargo.toml` (package `deltu` 0.1.0, edition 2024;
  deps: serde, serde_json; dev-dep: criterion), `Cargo.lock`, `src/`
  (`lib.rs`, `main.rs`, `event/`, `processing/`), `benchmarks/processing.rs`,
  `target/` (ignored), plus the original `context/` and `AGENT.md`.
- Unit 01 was implemented strictly within spec scope: no dependencies, no
  async runtime (decision 004), no event-engine code. The next unit begins
  with its spec, per the workflow rules.
- Git: Units 00–01 on `feat/01-rust-foundation`; Units 02–03 on
  `feat/02-event-model` and `feat/03-processing-core`. Direct pushes from
  the coding shell lack HTTPS credentials; sync via the client or a
  credentialed environment.
