# 004 — Async Runtime Deferred Until Required

Status: Accepted · Date: 2026-09-25

## Decision

The core engine starts fully synchronous. An async runtime (tokio or
equivalent) is introduced only when continuous processing — network input
adapters, concurrent workers, timers — actually requires it, and only in the
unit whose specification justifies it.

## Reason

The first units (event model, processing stages, state, rules) are pure
data transformations that are simpler, faster to test, and deterministic
without a runtime. Adding tokio at `cargo init` time would pull a large
dependency tree before any feature needs it, complicate testing (async
tests, time control), and violate the dependency rule — every dependency
must have a reason (build plan §19). The engine's eventual shape (bounded
channels, workers, graceful shutdown) is well understood and maps cleanly
onto a runtime when the HTTP and MQTT units arrive.

## Alternatives Considered

* **Tokio from day one** — avoids a later "asyncification" refactor, but
  front-loads a heavy dependency and async complexity into units that are
  pure synchronous logic.
* **Thread-per-stage with std channels** — a middle path; std threads plus
  bounded `std::sync::mpsc`/`crossbeam` channels can serve the first
  continuous-processing unit without full async. Re-evaluate when the
  runtime unit is specified.

## Consequences

* Units 01–05 are synchronous and block-free; `cargo test` stays trivial.
* When a runtime is introduced (Unit 07/08 timeframe), the event model,
  processing stages, state, and rules are already stable, tested interfaces,
  so the change is localized to the runtime boundary — not a redesign.
* Library code must avoid blocking calls inside future-processing paths
  once async exists (code standards); until then, blocking is the entire
  model and is fine.
