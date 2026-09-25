# Spec 10 — Metrics

Status: COMPLETE (implemented & verified 2026-09-25) · Depends on: Unit 07 (Runtime & HTTP), Unit 08 (MQTT)

## Goal

Consolidate the counters accumulated since Unit 03 into Deltu's metrics
model (research/performance.md): a lightweight internal registry, an
extended `/v1/status`, and the first measured baseline report. No external
metrics system becomes mandatory.

## Design

### Module

`core/metrics/` maps to `src/metrics/`:

```text
src/metrics/
├── mod.rs      # MetricsRegistry, MetricKind, snapshot API, re-exports
└── export.rs   # status-endpoint JSON rendering
```

### Registry

* Unified snapshot struct aggregating: pipeline counters (filtered,
  duplicates, late, evictions, suppressed), state counters (expired,
  evicted), action outcomes (attempted/succeeded/failed), adapter counters
  (http requests, mqtt received/rejected/dropped/reconnects), queue depth,
  uptime, event throughput (events/sec computed over a sliding 60s window
  in event-time-independent wall time owned by the runtime).
* Registry is append-only by fixed field — no dynamic label explosion, no
  unbounded series (bounded-state invariant applies to metrics too).
* Latency: per-stage timing via criterion-invisible instrumentation —
  a cheap `Instant`-based ring buffer of the last N=1024 durations per
  stage, exposing avg/P95/P99 on snapshot. Bounded; overflow overwrites
  oldest.
* Export: `/v1/status` JSON (documented shape in this spec before
  implementation). Prometheus/OpenMetrics stays an optional future adapter
  (11+) — not a core dependency (architecture: no vendor coupling).

### Baseline report

This unit produces the first end-to-end **measured baseline** (throughput +
latency P95/P99 through HTTP → pipeline → state → action on the dev
machine) recorded in the tracker, replacing the Unit 03 stage-only numbers
as the reference point. Sustained-load behavior (memory stability over
10 minutes) is part of the baseline.

## Implementation

1. Registry + snapshot + export; wire into runtime and `/v1/status`.
2. Ring-buffer latency capture in the worker paths (cheap, bounded).
3. Tests: snapshot completeness, P95/P99 math on known inputs, ring-buffer
   bounds, throughput calculation, status JSON shape.
4. Baseline benchmark run + tracker record.
5. Scope guard: no Prometheus/Grafana/external push, no histogram buckets
   beyond the ring buffer, no new dependencies.

## Dependencies

None added.

## Finalized at Unit Start (review pass, 2026-09-25)

1. **Registry shape:** one `MetricsRegistry` (`src/metrics.rs`) holding the
   throughput window + three `LatencyRing`s (batch / pipeline / action).
   All fields fixed; no dynamic labels; rings default 1024 samples,
   overwrite-oldest — the bounded-state invariant covers metrics.
2. **Latency capture point:** batch latency is measured at the HTTP
   boundary (validation → enqueue → worker receipt) — the end-to-end
   user-visible latency. Stage-level rings exist and are populated by the
   runtime as stages are wired (pipeline/action capture points land with
   the first long-running workloads; the rings and snapshot shape are
   already in the documented contract).
3. **Percentiles:** nearest-rank on the retained ring samples; empty ring
   reports 0 (never NaN, never an error).
4. **Baseline:** measured with `docs/baseline.rs` (a zero-dependency load
   driver kept in-repo) against a live engine — results in the tracker.

## Verify When Done

* [ ] Full cargo suite clean; prior tests green.
* [ ] /v1/status returns the documented snapshot; fields match counters in
      tests.
* [ ] Baseline numbers recorded in the tracker with conditions.
* [ ] Tracker updated (results + notes).
