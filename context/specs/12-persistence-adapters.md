# Spec 12 — Persistence Adapters (Optional Layer)

Status: DRAFT (pre-drafted on request; finalize at unit start) · Depends on: Units 03–10 (complete deterministic core) · Optional: engine runs fully without this unit

## Goal

Introduce optional persistence behind adapters without touching the
in-memory processing path (decision 002; invariants 2, 10, 12, 13):
(a) local snapshot/recovery for restarts, (b) the PostgreSQL historical
adapter for deployments that need analytics. Both are strictly opt-in via
configuration; default builds run exactly as before.

## Design

### Module

```text
adapters/                    # workspace-external or crate features, decided at unit start
├── persistence/             # trait + local file snapshot
│   ├── mod.rs               # PersistenceAdapter trait, SnapshotConfig
│   └── local_file.rs        # atomic file snapshots (tempfile + rename)
└── postgres/                # feature "postgres"
    └── mod.rs               # historical event sink
```

### Contract

```rust
pub trait PersistenceAdapter: Send {
    fn snapshot(&self, state: &StateStore) -> Result<(), PersistenceError>;
    fn restore(&self) -> Result<Option<Snapshot>, PersistenceError>;
}

pub trait HistoricalSink: Send {
    fn write_batch(&mut self, events: &[Event]) -> Result<(), PersistenceError>;
}
```

* **Local snapshots (feature "snapshots"):** periodic, atomic
  (write-temp-then-rename), bounded size (state is bounded by design —
  spec 04), restored on startup only when configured. Crash-consistency
  argument documented; snapshot cadence runtime-configured.
* **PostgreSQL (feature "postgres") via `sqlx`:** historical event storage
  only — the *runtime state path never blocks on it*. Writes are batched
  through a bounded channel with drop-and-count on overflow; connection
  failures degrade to counting (observable, never fatal, invariant 8
  pattern). Schema migrations embedded and versioned; documented in this
  spec before implementation.
* Nothing in the core crate may call these adapters directly; wiring lives
  in the runtime configuration layer only — enforced by module visibility
  and tested by a compile-time structure test.
* Retention, downsampling, and query APIs are out of scope (post-MVP).

## Implementation

1. Trait definitions + local file snapshots + tests (round-trip, atomicity
   on simulated crash mid-write, corrupted-file rejection).
2. Postgres adapter behind feature flag with testcontainers-style CI test
   (optional, documented); batch + backpressure + failure-degradation tests
   are broker-free and mandatory.
3. Runtime config wiring (`persistence:` section), `/v1/status` fields for
   snapshot age and sink backlog/drops.
4. Scope guard: no ORM, no state-dependency of processing on persistence,
   no mandatory feature flags, no cloud storage.

## Dependencies

Feature-gated only: `sqlx` (postgres, runtime-tokio) + `tempfile` (dev).
Default build unchanged.

## Verify When Done

* [ ] Default build unchanged: no persistence code paths active, prior tests
      green.
* [ ] Feature builds compile; snapshot round-trip and crash-atomicity tested.
* [ ] Postgres failure degradation proven (engine continues, drops counted).
* [ ] Tracker updated (results + notes).
