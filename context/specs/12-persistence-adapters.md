# Spec 12 — Persistence Adapters (Optional Layer)

Status: COMPLETE (implemented & verified 2026-09-25; finalized semantics below) · Depends on: Units 03–10 (complete deterministic core) · Optional: engine runs fully without this unit

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

## Finalized at Unit Start (review pass, 2026-09-25)

1. **Location:** in-crate `src/persistence/`, no cargo features, no new
   dependencies — stricter than the pre-draft's feature-gate wording and
   correct per the dependency rule (the reference adapter needs only
   `std` + `serde_json`, already present).
2. **Snapshot fidelity:** restore preserves `previous_value`, `updates`,
   and `updated_at_ms` exactly (`restore_entry` bypasses `observe`), and
   is capacity-capped by the store's own eviction — restore cannot
   violate the bounded-state invariant.
3. **Restore failure semantics:** a corrupt/unreadable snapshot is fatal
   at startup (refusing to run with silently-lost state beats pretending);
   snapshot *write* failures at runtime degrade to a counter
   (`persistence.snapshot_failures` in `/v1/status`) — invariant 8.
4. **Expiry interaction (documented, tested live):** restored entries keep
   their original timestamps; a snapshot older than `expire_after_ms` is
   expired by the first periodic pass after restore. Correct-by-design:
   persistence restores *state*, not freshness.
5. **HistoricalSink ships as contract + reference implementation:**
   `InMemoryHistoricalSink` is bounded, drops-and-counts on overflow, and
   records failed batches — the broker-free degradation tests the spec
   marks mandatory run against it. The PostgreSQL adapter is deferred
   until a deployment needs it: `sqlx` cannot be justified now (no
   postgres target to test against), and the trait is the seam — the
   adapter is an implementation detail behind it. Structure test pins the
   wiring: persistence types are referenced *only* from the runtime
   config layer, never from the processing path.

## Verified (2026-09-25)

* 166 tests pass (161 prior + 5 new: round-trip fidelity, missing-file,
  crash-mid-write atomicity, corruption rejection, version rejection,
  capacity cap, sink degradation, config validation).
* Live end-to-end: snapshot written on cadence, process killed, restart
  restored `state entries: 1 | restored: 1` via `/v1/status`.
* `cargo fmt --check` clean, `cargo clippy --all-targets` 0 warnings,
  default build unchanged (no new dependencies).

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

* [x] Default build unchanged: no persistence code paths active, prior tests
      green.
* [x] Feature builds compile; snapshot round-trip and crash-atomicity tested.
* [x] Postgres failure degradation proven (engine continues, drops counted).
      (Against the reference sink; the adapter itself is deferred — see
      Finalized #5.)
* [x] Tracker updated (results + notes).
