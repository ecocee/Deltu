# Spec 07 — Runtime & HTTP

Status: COMPLETE (implemented & verified 2026-09-25) · Depends on: Units 03–06 (Pipeline, State, Rules, Actions contract)

## Goal

Introduce the async runtime and the first real input adapter: a
language-agnostic HTTP/JSON ingestion API (research/http.md). This is the
unit where decision 004 is retired — continuous processing now genuinely
requires tokio — and where Deltu first runs as a long-lived service.

## Design

### Module

`core/runtime/` and `core/input/http/` map to:

```text
src/runtime/       # startup, config load, worker loop, graceful shutdown
src/input/http.rs  # axum routes; validation only — no processing logic
```

### Runtime

* Adds `tokio` (rt-multi-thread, macros, signal) — the documented retirement
  of decision 004: bounded workers, bounded channel between HTTP handlers
  and the processing loop, graceful shutdown on SIGINT/SIGTERM draining
  in-flight events.
* YAML/JSON/env configuration (`serde` already present; add `serde_yaml` —
  last config-layer dependency) with explicit validation and actionable
  errors; no silent defaults beyond the documented ones.
* The synchronous pipeline/state/rules core (Units 03–05) runs inside a
  dedicated worker task fed by a **bounded channel** with a configured
  overflow policy (block/drop-new) and drop counters — the backpressure
  model from research/event-processing.md finally has a consumer.
* LogAction migrates to `tracing` (subscribers replace ad-hoc lines).
* `deltu` binary becomes a real service: `deltu run --config <path>` (plus
  `--version`, `--help`); CLI surface stays minimal per ui-context.md.

### HTTP API (v0, documented in this spec before implementation)

* `POST /v1/events` — batch of 1..=1000 events, canonical JSON shape from
  spec 02. 202 with per-event results (`accepted`/`rejected` + reason);
  400 malformed; 413 too large; 429 queue full; 503 shutting down.
* `GET /health` — liveness without touching engine state.
* `GET /v1/status` — counters (pipeline, state, actions, queue depth),
  uptime, version.
* Handlers: parse → validate → enqueue. No processing in handlers
  (invariant 7); every response shape documented here; request id in errors.

### Dependencies (each per the dependency rule)

`tokio`, `axum` (0.8.x line), `tower` + `tower-http` (timeout/limit layers
only), `tracing` + `tracing-subscriber`, `serde_yaml`. All pure Rust or
tokio-native; ARM64-safe (research/edge.md).

## Implementation

1. Runtime: config load/validate, worker task + bounded channel, shutdown
   signal handling, tracing setup.
2. HTTP: routes, extractors with payload-size and content-type guards, error
   envelope, integration of dispatch into the worker.
3. Wire rules→actions end to end (a rule match produces a log action).
4. Tests: config errors; channel overflow policy (drop counted); HTTP
   integration tests via `tower::ServiceExt` (valid batch, invalid event,
   oversized, queue-full, health); graceful shutdown drains without loss.
5. Benchmark deferred to Unit 10 (runtime metrics become measurable there).
6. Scope guard: no MQTT, no SDKs, no persistence, no AI, no auth beyond
   optional static API key check.

## Finalized at Unit Start (review pass, 2026-09-25)

Decisions made during implementation, recorded here:

1. **Decision 004 retired exactly as planned**: `tokio` 1.53 (rt-multi-thread,
   macros, signal, time) + `axum` 0.8.9 + `serde_yaml` 0.9 + dev-only
   `tower` (util) are the dependency set. `tracing`/`tracing-subscriber`
   are **deferred to the CLI unit (09)** with the LogAction migration — the
   log action already produces structured lines, so the logging framework
   adds nothing in this unit (dependency rule).
2. **Config layer**: `RuntimeConfig` (YAML/JSON file, `deny_unknown_fields`,
   all-optional sections defaulting to the documented module configs) with
   `DELTU_HTTP_BIND` / `DELTU_QUEUE_CAPACITY` env overrides and
   referential validation (rule → action must resolve; unique ids).
   `PipelineConfig`/`StateConfig`/rule/action definitions gained serde
   derives for this (existing dependencies only).
3. **Error envelope completeness**: the events handler takes raw bytes and
   parses explicitly, so *malformed JSON* also returns the documented
   envelope (the `Json` extractor's default rejection would have bypassed
   it). Success returns **202** (accepted for processing) as documented.
4. **Queue depth observability**: an mpsc sender cannot read channel
   length; the worker reports the last-known depth on the core, surfaced
   by `/v1/status`.
5. **Worker shape**: the core (Units 03–06) is owned behind a `std::sync::Mutex`
   and driven by a tokio worker task from a bounded mpsc queue
   (`try_send` → 429 on overflow) with a periodic 30s state-expiration
   tick. Graceful shutdown on SIGINT/SIGTERM via `with_graceful_shutdown`.
6. **Fixed during verification**: `get_status` originally double-locked the
   core mutex (self-deadlock — caught by the test suite hanging);
   rewritten as a single lock acquisition. Numeric events accumulate into
   open windows by design and do not fire rules until a summary closes —
   immediate rule firing happens on passthrough events; worker tests
   corrected to match this documented dataflow.

## Verify When Done

* [ ] Full cargo suite clean (check/build/test/run/fmt --check/clippy).
* [ ] `cargo run` starts the service; /health answers; Ctrl-C exits cleanly
      within the drain timeout.
* [ ] HTTP integration tests cover every documented status code.
* [ ] New dependencies limited to the table above; tracker updated.
