# Progress Tracker

Update this file after every meaningful implementation change.

## Current Phase

- All 14 units **COMPLETE** — MVP build plan delivered.

## Current Goal

- Post-MVP: drive the release workflow on a `v0.1.0` tag; extend SDKs /
  adapters as requirements arrive (infra on demand).

## Completed

- Unit 14 — Packaging & Deployment: **COMPLETE** (2026-09-25, per
  `context/specs/14-packaging-deployment.md`, on `feat/09-cli`)
  - Multi-stage Dockerfile: distroless/cc runtime, non-root, shell-free
    `HEALTHCHECK` via the engine's own `deltu health` CLI; AI excluded
    from the default image (spec 11 gating).
  - `.github/workflows/ci.yml`: fmt/clippy/test per PR + Docker smoke
    (build, run, real `/health` probe).
  - `.github/workflows/release.yml`: verify → build Linux x86_64,
    Linux ARM64 (`cross`), macOS ARM64 → `SHA256SUMS` → GitHub Release.
  - `docker/compose.yaml` + mosquitto config + validated `engine.yaml`:
    two-container MQTT demo (no Kubernetes/Helm — infra on demand).
  - `examples/demo.sh`: scripted sensor→HTTP→pipeline→state→rule→action
    proof, verified live twice (structured log line fired,
    `accepted [true,true,true]`, `actions_fired 1`).
  - `docs/deploy.md`: self-hosting guide matching exactly what the
    workflows produce.
  - Release numbers (Apple Silicon, stripped): binary 5.2 MB, cold start
    to `/health` ≈ 0.65 s; Unit 10 baseline: ≈77k events/s loopback.

- Unit 13 — SDKs: **COMPLETE** (2026-09-25, per
  `context/specs/13-sdks.md`, on `feat/09-cli`)
  - `sdk/python`: `deltu` package — `DeltuClient` (send_events/
    send_event/health/status), pre-flight `Event` validation mirroring
    spec 02, typed 400/413/429/503 error classes, opt-in 429/503 retries,
    `StatusSnapshot`; 12/12 tests incl. live e2e against a real engine.
  - `sdk/typescript`: `@deltu/client` — same surface, strict tsc (no
    `any`, `exactOptionalPropertyTypes`), built-in fetch, zero runtime
    deps; 12/12 tests incl. live e2e.
  - `.github/workflows/sdk.yml` + `.github/deltu-ci.yaml`: CI starts a
    live `deltu run` and runs both suites against it (first true e2e of
    the public contract).

- Unit 12 — Persistence: **COMPLETE** (2026-09-25, per
  `context/specs/12-persistence-adapters.md`, on `feat/09-cli`)
  - `src/persistence/`: `PersistenceAdapter` + `HistoricalSink` traits,
    `LocalFileAdapter` (atomic temp+rename snapshots, version-checked
    restore), `InMemoryHistoricalSink` (bounded, drop-and-count reference
    implementation), `apply_snapshot` + `PersistenceCounters`.
  - `StateStore::restore_entry`: full-fidelity restore (previous value,
    update count, timestamp preserved), capacity-capped by store eviction.
  - Runtime wiring: `persistence:` config section (opt-in; validated),
    restore-on-startup, worker-owned periodic snapshots, failure
    degradation to `persistence.*` counters in `/v1/status`.
  - Postgres adapter deferred behind the `HistoricalSink` seam (no testable
    target; structure test pins the wiring). 166 tests, clippy/fmt clean,
    live restart-restore verified end-to-end.
- Unit 11 — Local AI: **COMPLETE** (2026-09-25, per
  `context/specs/11-local-ai.md`, on `feat/09-cli` — units 10–14 on one
  branch per user instruction)
  - `AiProvider` trait + `AiManager` (src/ai.rs): invocation policy
    enforced by construction — the manager lives inside ActionDispatcher
    and only `ActionKind::Ai` requests route through it (tested that log
    actions never touch AI).
  - Usage metrics mandatory: calls/succeeded/failed/tokens/latency, in
    `/v1/status` under `actions.ai`.
  - No provider in the default build: `Ai` actions fail fast as counted
    outcomes when unconfigured; the scripted in-memory provider covers
    tests/CI without any model; ONNX/llama.cpp runtimes remain future
    cargo features (zero AI crates in Cargo.toml — stricter than the
    pre-draft, per the dependency rule).
  - Latency benchmark skipped (documented deviation): the scripted
    provider is instant; a real-runtime bench arrives with the feature
    builds.
  - Dependencies: none added.
  - Verification results: `cargo test` 155 passed / 0 failed (146 prior +
    9 new); check/build/fmt/clippy --all-targets clean.
- Unit 10 — Metrics: **COMPLETE** (2026-09-25, per
  `context/specs/10-metrics.md`, on `feat/09-cli` — units 10–14 proceed
  without new branches per user instruction)
  - `MetricsRegistry` (src/metrics.rs): fixed-field registry;
    `ThroughputWindow` (sliding 60s, per-second buckets, evicting,
    non-decaying total); `LatencyRing` ×3 (batch/pipeline/action),
    capacity 1024, overwrite-oldest, nearest-rank percentiles, empty = 0.
  - `/v1/status` now includes the documented `metrics` block
    (events_per_sec, events_total, latency_ms per stage with avg/p95/p99).
  - No new dependencies.
  - Verification results: `cargo test` 146 passed / 0 failed (140 prior +
    6 new: percentile math, ring bounds, window eviction, snapshot shape);
    check/build/fmt/clippy all clean.
  - Baseline (docs/baseline.rs driver, 300 batches × 50 events, dev build,
    Apple M5, loopback, idle machine): **76,905 events/s, 0 failures,
    latency avg 0.1 ms, P95 1 ms, P99 1 ms, max 2 ms**; engine stayed
    responsive and shut down cleanly. Documented starting point for this
    hardware only.
- Unit 09 — CLI: **COMPLETE** (2026-09-25, per
  `context/specs/09-cli.md`, branch `feat/09-cli`)
  - Binary name resolved: `deltu` (open question closed).
  - `deltu run [--config]` / `check` / `status [--url]` / `health [--url]` /
    `--version` via clap 4.6 (derive); exit codes 0/1/2 implemented and
    tested; `status`/`health` are thin documented-API clients (zero-dep
    hand-rolled HTTP/1.1 GET — no client crate, dependency rule).
  - Incident & fix: an internally-tagged serde representation attempted on
    the recursive `Trigger`/`Condition` enums caused rustc E0275 derive
    overflow and the reported `cargo test` slowdown; reverted to external
    tagging (YAML `!Tag` syntax), compile time back to ~2s. Documented in
    spec 09.
  - Dependencies: clap only (existing HTTP/serde stack reused).
  - Verification results: `cargo check --all-targets` clean; `cargo build`
    ok; `cargo test` 140 passed / 0 failed (135 prior + 5 new); `cargo fmt
    --check` clean; `cargo clippy --all-targets` 0 warnings; live CLI:
    check→0, health→healthy/0, status→JSON/0, SIGTERM clean.
- Unit 08 — MQTT: **COMPLETE** (2026-09-25, per
  `context/specs/08-mqtt.md`, branch `feat/08-mqtt`)
  - Open question resolved: MQTT protocol version is a config enum
    (`Transport`: Mqtt5 default, Mqtt31) — divergence point in place.
  - `src/input/mqtt/`: config (broker URL, client id, topics, QoS with
    closed-enum validation, keep-alive, session expiry), conversion
    (documented topic→source/kind mapping; raw JSON scalars → scalar
    payloads, objects → structured; non-JSON rejected and counted,
    never coerced; adapter-injected timestamps; per-message ids), and the
    eventloop task (subscriptions at startup, rumqttc internal reconnection
    with counted reconnects, bounded drop-new delivery into the engine
    queue — broker failures never propagate, invariant 8).
  - Config: `mqtt:` section gated by `enabled` (default false — an absent
    section never connects); MQTT counters surfaced in `/v1/status`.
  - Dependency added: `rumqttc` 0.25.1 (pure Rust, ARM64-safe per edge
    research).
  - Verification results: `cargo check --all-targets` clean; `cargo build`
    ok; `cargo test` 135 passed / 0 failed (123 prior + 12 new — conversion
    and URL tests run fully without a broker); `cargo run -- --version`;
    `cargo fmt --check` clean; `cargo clippy --all-targets` 0 warnings.
  - Live-broker integration test remains documented-and-optional per spec
    (no broker in CI); all unit behavior verified broker-free.
- Unit 07 — Runtime & HTTP: **COMPLETE** (2026-09-25, per
  `context/specs/07-runtime-http.md`, branch `feat/07-runtime-http`)
  - Decision 004 retired: tokio 1.53.1 + axum 0.8.9 + serde_yaml 0.9.34 +
    dev-only tower (util) added; `tracing` deferred to Unit 09 (log lines
    already structured).
  - `RuntimeConfig`: YAML/JSON file (deny_unknown_fields), env overrides
    (`DELTU_HTTP_BIND`, `DELTU_QUEUE_CAPACITY`), referential validation
    (rule → action, unique ids); PipelineConfig/StateConfig/rule/action
    definitions now serde-derived.
  - Worker: EngineCore (pipeline → state → rules → actions) behind a
    std::sync::Mutex, driven by a tokio task from a bounded mpsc queue;
    30s expiration tick; graceful shutdown on SIGINT/SIGTERM.
  - HTTP API v0 live: `POST /v1/events` (202 accepted; 400 with envelope
    incl. malformed JSON and named-event-index validation errors; 413
    body-limit; 429 queue-full; 404 unknown route), `GET /health`,
    `GET /v1/status` (pipeline/state/action counters, queue depth,
    uptime, version).
  - CLI v0: `deltu run [--config <path>]`, `--version`, `--help`.
  - Fixed during verification: get_status double-lock self-deadlock
    (caught by hung test suite) rewritten as single lock acquisition.
  - Verification results: `cargo check --all-targets` clean; `cargo build`
    ok; `cargo test` 123 passed / 0 failed (108 prior + 15 new, incl. the
    full HTTP status-code suite); `cargo run -- --version` → `deltu 0.1.0`;
    `cargo fmt --check` clean; `cargo clippy --all-targets` 0 warnings.
  - Live smoke test passed: service started with defaults, /health ok,
    valid event accepted into state, invalid event rejected with envelope,
    /v1/status correct, SIGTERM shutdown clean.
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

- Unit 12 — Persistence: finalize `context/specs/12-persistence-adapters.md`,
  implement and verify. Then units 13–14 in order (same branch policy).

## Open Questions

- CLI binary name: RESOLVED in Unit 09 — shipped as `deltu` (crate, binary,
  and docs share one name; the `plan-c` examples in older docs are
  historical).
- MQTT protocol version default: RESOLVED in Unit 08 — `Transport` config
  enum (Mqtt5 default, Mqtt31 for older brokers); divergence point in place.

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
  deps: serde, serde_json, tokio, axum, serde_yaml, rumqttc, clap;
  dev-deps: criterion, tower), `Cargo.lock`, `src/` (`lib.rs`, `main.rs`,
  `cli.rs`, `actions/`, `event/`, `input/`, `processing/`, `rules/`,
  `runtime/`, `state/`), `benchmarks/{processing,state,rules}.rs`,
  `target/` (ignored), plus the original `context/` and `AGENT.md`.
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
  `feat/06-actions`; Unit 07 on `feat/07-runtime-http`; Unit 08 on
  `feat/08-mqtt`; Unit 09 on `feat/09-cli`. Direct pushes from the coding
  shell lack HTTPS credentials; sync via the client or a credentialed
  environment.
