# Progress Tracker

Update this file after every meaningful implementation change.

## Current Phase

- Unit 07 — Runtime & HTTP: **NOT STARTED** (pre-drafted spec 07 to be
  finalized first; retires decision 004)

## Current Goal

- Finalize `context/specs/07-runtime-http.md` at unit start, then implement
  the async runtime, bounded-channel worker, and the documented HTTP API.

## Completed

- Unit 06 — Actions: **COMPLETE** (2026-09-25, per
  `context/specs/06-actions.md`, branch `feat/06-actions`)
  - `ActionDispatcher` with failure isolation (invariant 8): executes every
    request, collects one `ActionOutcome` per request, never panics or
    aborts on failure — proven by a deliberately-failing executor test.
  - Contract: `ActionExecutor` trait (the seam Units 07/08 use for
    webhook/MQTT executors), `ActionDefinition { id, kind }`,
    `ActionKind::Log`, `ActionSummary`, `ActionCounters`.
  - Log action: structured JSON line to stderr, caller-injected timestamp
    (clock-free), optional `{rule_id}`/`{payload}` template, line returned
    in the outcome for direct assertion.
  - Finalized at unit start (documented in spec): executor abstraction,
    `dispatch(requests, now_ms)` signature, stderr sink + returned line,
    `ActionRequest` stays defined in Unit 05 (no duplication).
  - No benchmark (justified deviation per spec: dispatch is
    log-formatting-bound; revisit with Unit 10 metrics).
  - Dependencies: none added.
  - Verification results: `cargo check --all-targets` clean; `cargo build`
    ok; `cargo test` 108 passed / 0 failed (98 prior + 10 new); `cargo run`
    → `deltu 0.1.0`; `cargo fmt --check` clean; `cargo clippy
    --all-targets` 0 warnings.
- Unit 05 — Rules: **COMPLETE** (2026-09-25, per
  `context/specs/05-rules.md`, branch `feat/05-rules`)
  - Finalized at unit start: Event triggers match either input shape;
    State triggers fire while their key exists (absence rules → runtime
    unit); type mismatches compare `false` incl. cross-type `Ne`;
    ordering ops numeric-only (text/boolean Eq/Ne); suppression state
    bounded by construction (one slot per rule); `RuleError` collapsed
    into `RuleConfigError` (evaluation is total).
  - `RuleEngine::evaluate(EvalInput) -> Vec<ActionRequest>`: triggers
    (Event family / State key), typed condition trees (All/Any/Not/
    Comparison/Exists), fields (EventValue/State/Aggregation stat),
    closed operator set, event-time once-per-window suppression with
    counter, id-ordered deterministic output, serde-derived definitions.
  - `kind_matches_family` extracted to `crate::processing` and shared by
    filter and rule triggers (one implementation, two consumers).
  - Dependencies: none added (serde/serde_json/criterion reused).
  - Verification results: `cargo check --all-targets` clean; `cargo build`
    ok; `cargo test` 98 passed / 0 failed (82 prior + 16 new); `cargo run`
    → `deltu 0.1.0`; `cargo fmt --check` clean; `cargo clippy --all-targets`
    0 warnings; `cargo bench --bench rules` ran.
  - Benchmark (criterion 0.8.2, Apple M5, idle dev machine, bench profile):
    `rules_evaluate_100` — 5.75 µs per evaluation of 100 mixed-condition
    rules (~57 ns/rule). Documented starting point for this hardware only.
- Unit 04 — State Engine: **COMPLETE** (2026-09-25, per
  `context/specs/04-state-engine.md`, branch `feat/04-state-engine`)
  - `StateStore` keyed by `(source, kind)` mirroring the pipeline keying;
    `StateValue` mirrors `Payload` (no serde, per spec); entries carry
    `previous_value`, `updated_at_ms`, `updates`.
  - `observe(&Output)` write path: raw events store their payload value at
    `event.timestamp`; aggregated summaries store the window mean at
    `window_end_ms`; re-observation sets `previous_value`, increments
    `updates`, refreshes recency.
  - Narrow read interface: `get`, `entries()` sorted by key for
    deterministic downstream evaluation, `len`/`is_empty`/`counters`.
  - Clock-free periodic expiration: `expire(now_ms)` removes and returns
    entries with `age >= expire_after_ms`; boundary expires; negative ages
    (clock skew) never expire; `None` disables.
  - Capacity bounding: LRU-by-last-update eviction (stalest victim, ties by
    insertion order) — deliberately different from Unit 03's FIFO dedup.
  - Benchmarks: `benchmarks/state.rs` registered as a second explicit
    `[[bench]]` target; both spec cases ran.
  - Verification results: `cargo check --all-targets` clean; `cargo build`
    ok; `cargo test` 82 passed / 0 failed (61 prior + 21 new);
    `cargo run` → `deltu 0.1.0`; `cargo fmt --check` clean;
    `cargo clippy --all-targets` 0 warnings; no new dependencies.
  - Benchmark results (criterion 0.8.2, Apple M5, idle dev machine, bench
    profile, 100 samples):
    - `state_observe_100_events`: 7.35 µs/100-event batch (~74 ns/event).
    - `state_expire_scan_10k`: 885 µs full-population scan at the default
      10_000-entry capacity (~89 ns/entry; removal + return of all 10k).
    Numbers are the documented starting point for this hardware only.
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

- None.

## Next Up

- Unit 07 — Runtime & HTTP: finalize the pre-drafted
  `context/specs/07-runtime-http.md`, then implement and verify. Remaining
  pre-drafts (08–14) are finalized at their unit start per the workflow
  (see `context/specs/README.md`).

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
  (`lib.rs`, `main.rs`, `actions/`, `event/`, `processing/`, `rules/`,
  `state/`), `benchmarks/{processing,state,rules}.rs`, `target/`
  (ignored), plus the original `context/` and `AGENT.md`.
- Unit 01 was implemented strictly within spec scope: no dependencies, no
  async runtime (decision 004), no event-engine code. The next unit begins
  with its spec, per the workflow rules.
- Specs 05–14 pre-drafted 2026-09-25 as separate files with a
  `specs/README.md` status index; all marked PRE-DRAFTED (finalize at unit
  start). Spec 07 records the deliberate retirement of decision 004;
  spec 09 records the recommended resolution of the binary-name question
  (`deltu`).
- Git: Units 00–01 on `feat/01-rust-foundation`; Units 02–03 (and specs
  04–14) on `feat/02-event-model` / `feat/03-processing-core`; Unit 04 on
  `feat/04-state-engine`; Unit 05 on `feat/05-rules`; Unit 06 on
  `feat/06-actions`. Direct pushes from the coding shell lack HTTPS
  credentials; sync via the client or a credentialed environment.
