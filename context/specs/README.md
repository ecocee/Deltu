# Specifications Index

Per-unit implementation specifications. The specification for a unit is the
source of implementation instructions for that unit (build plan §16); work
one unit at a time (§26). Specs marked PRE-DRAFTED were written ahead of
their unit on request and must be re-verified and finalized at unit start.

| Spec | Unit | Status |
| ---- | ---- | ------ |
| [00-build-plan.md](00-build-plan.md) | — Master build plan | AUTHORITATIVE |
| [01-rust-foundation.md](01-rust-foundation.md) | 01 — Rust Foundation | COMPLETE |
| [02-event-model.md](02-event-model.md) | 02 — Event Model | COMPLETE |
| [03-processing-core.md](03-processing-core.md) | 03 — Processing Core | COMPLETE |
| [04-state-engine.md](04-state-engine.md) | 04 — State Engine | COMPLETE |
| [05-rules.md](05-rules.md) | 05 — Rules | COMPLETE |
| [06-actions.md](06-actions.md) | 06 — Actions | COMPLETE |
| [07-runtime-http.md](07-runtime-http.md) | 07 — Runtime & HTTP | COMPLETE |
| [08-mqtt.md](08-mqtt.md) | 08 — MQTT | COMPLETE |
| [09-cli.md](09-cli.md) | 09 — CLI | COMPLETE |
| [10-metrics.md](10-metrics.md) | 10 — Metrics | COMPLETE |
| [11-local-ai.md](11-local-ai.md) | 11 — Local AI (optional) | COMPLETE (boundary; runtimes feature-gated future) |
| [12-persistence-adapters.md](12-persistence-adapters.md) | 12 — Persistence adapters (optional) | COMPLETE |
| [13-sdks.md](13-sdks.md) | 13 — SDKs | COMPLETE |
| [14-packaging-deployment.md](14-packaging-deployment.md) | 14 — Packaging & deployment | PRE-DRAFTED |

## Notes

* Units 11+ are the optional layer (AI, persistence): the engine is fully
  functional without them, per decision 003 and the build plan's dependency
  and infrastructure rules.
* Spec 07 retires decision 004 (deferred async runtime) — the runtime
  dependency landed there, deliberately.
* Pre-drafted specs are inputs to their unit's Definition of Ready, not
  substitutes for the review pass at unit start.
