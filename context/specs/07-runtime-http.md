# Spec 07 — Runtime & HTTP

Status: DRAFT (pre-drafted on request; finalize at unit start) · Depends on: Units 03–06 (Pipeline, State, Rules, Actions contract)

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

## Verify When Done

* [ ] Full cargo suite clean (check/build/test/run/fmt --check/clippy).
* [ ] `cargo run` starts the service; /health answers; Ctrl-C exits cleanly
      within the drain timeout.
* [ ] HTTP integration tests cover every documented status code.
* [ ] New dependencies limited to the table above; tracker updated.
