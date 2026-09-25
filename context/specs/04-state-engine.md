# Spec 04 — State Engine

Status: DRAFT (ready to implement) · Depends on: Unit 03 (Processing Core)

## Goal

Implement Deltu's in-memory **current-state store**: the keyed, bounded,
expiring snapshot of "what is true now" that the rule engine (Unit 05) and
downstream actions will read. This unit implements the store, its write path
from pipeline outputs, its read interface, periodic expiration, and bounded
capacity. It does **not** implement rules, actions, persistence, the runtime
composition, or any metrics interface.

## Design

### Ownership and module boundary

`core/state/` in the architecture maps to the `state` module of the `deltu`
crate. Per decision 002 the store is memory-first: no database, no cache, no
filesystem, and nothing persists — runtime state and historical persistence
remain conceptually separate (architecture invariant 10).

```text
src/state/
├── mod.rs      # StateConfig, StateError, StateCounters, re-exports
└── store.rs    # StateStore, StateKey, StateEntry, StateValue
```

Public API re-exported at the crate root: `StateStore`, `StateConfig`,
`StateError`, `StateKey`, `StateEntry`, `StateValue`, `StateCounters`.

### What the store holds

One entry per key, where the key matches the pipeline's keying —
`(source, kind)`:

```rust
pub struct StateKey {
    pub source: String,
    pub kind: String,
}

/// The current scalar (or structured) value of a key. Mirrors `Payload`
/// without duplicating its serde tagging — state values are read by rules,
/// not reserialized (no serde derives here; see Dependencies).
pub enum StateValue {
    Numeric(f64),
    Text(String),
    Boolean(bool),
    Structured(serde_json::Value),
}

pub struct StateEntry {
    pub key: StateKey,
    /// Current value.
    pub value: StateValue,
    /// Immediate previous value, for transition rules (e.g. open → closed).
    /// Only the last transition is retained — never a history.
    pub previous_value: Option<StateValue>,
    /// Event-time (milliseconds) of the last update to this key.
    pub updated_at_ms: i64,
    /// Number of updates applied to this key since creation.
    pub updates: u64,
}
```

Numeric keys hold the **window mean** (`Aggregated.mean`) — the same
representative value the change deadband uses. Text/boolean/structured keys
hold the latest raw event's value.

### Write path — observing pipeline outputs

The store is fed by the caller with pipeline outputs; the runtime unit owns
the eventual wiring, and `observe` is the seam between Units 03 and 04:

```rust
pub fn observe(&mut self, output: &Output);
```

* `Output::Event(event)` — non-numeric events update their key directly:
  `value` from `event.payload` (`Numeric` cannot occur here — spec 03 routes
  all numerics into aggregation), `updated_at_ms = event.timestamp`.
* `Output::Aggregated(summary)` — `value = Numeric(summary.mean)`,
  `updated_at_ms = summary.window_end_ms` (the summary is current as of its
  window end).
* Creating an entry sets `updates = 1`, `previous_value = None`. Updating an
  existing entry moves the current value into `previous_value`, increments
  `updates`, and refreshes recency.

### Read path — the narrow rule interface

Deliberately small (code standards: keep runtime state out of
database-shaped APIs):

```rust
pub fn get(&self, key: &StateKey) -> Option<&StateEntry>;
pub fn entries(&self) -> Vec<&StateEntry>; // sorted by (source, kind)
pub fn len(&self) -> usize;
pub fn is_empty(&self) -> bool;
pub fn counters(&self) -> &StateCounters;
```

`entries()` returns a **sorted** Vec so downstream evaluation (rules in
Unit 05) is deterministic regardless of internal hash order — deterministic
behavior is a project standard wherever AI is not involved.

### Expiration — periodic, bounded, injected clock

Entries for sources that stop reporting must not accumulate forever
(`research/state.md`). Expiration is a **periodic scan-and-evict**, never
per-event, and the store itself is **clock-free** — the caller injects
`now`, keeping the module deterministic and unit-testable:

```rust
pub fn expire(&mut self, now_ms: i64) -> Vec<StateEntry>;
```

* An entry is expired when `now_ms − updated_at_ms ≥ expire_after_ms`.
* Expired entries are **removed** (removal, not marking — this is what
  bounds memory) and **returned** with their final values, so "went offline"
  logic in later units can use the last known state.
* Negative ages (`now_ms < updated_at_ms`, e.g. producer clock skew) are
  never expired. Multi-source clock-skew handling (watermark-style) is a
  documented non-goal of this unit, deferred to the runtime unit where the
  clock domain is actually known.
* `expire_after_ms: None` disables expiration; `expire()` then returns an
  empty Vec (capacity bounding still applies).
* The caller owns the cadence: the runtime unit will schedule periodic
  calls; until then callers invoke `expire` explicitly.

### Bounded capacity — LRU by last update

`max_entries` bounds the store regardless of input volume. When an insert
would exceed capacity, the **stalest** entry is evicted: smallest
`updated_at_ms`, ties broken by earliest insertion. Evicted entries are
dropped (not returned) and counted — stalest data is the least valuable.
This differs deliberately from spec 03's FIFO dedup cache: here recency has
meaning (fresh sensor data outranks old), so least-recently-updated is the
correct victim.

### Configuration and errors

```rust
pub struct StateConfig {
    pub max_entries: usize,            // default 10_000
    pub expire_after_ms: Option<i64>,  // default Some(300_000) — 5 minutes
}

pub struct StateCounters {
    pub expired: u64,
    pub evicted: u64,
}

pub enum StateError {
    /// max_entries must be > 0.
    InvalidCapacity(usize),
    /// expire_after_ms, when set, must be > 0.
    InvalidExpireAfter(i64),
}
```

Defaults are conservative for edge-class devices and are a documented
starting point, not performance claims. `StateError` implements actionable
`Display` and `std::error::Error` (spec 01 conventions); validation happens
once in `StateStore::new`; no `unwrap()`/`expect()` in production paths.

### Bounded-state invariants (tested, not aspirational)

The store can never hold more than `max_entries` entries regardless of
input volume, and expiration is bounded work over bounded state. Both
properties are covered by tests — this is the unit's core correctness
claim, same as spec 03.

## Implementation

1. `src/state/mod.rs` — `StateConfig` (+ `Default`), `StateError`,
   `StateCounters`, re-exports of the store types.
2. `src/state/store.rs` — `StateKey`, `StateValue`, `StateEntry`,
   `StateStore` with `new/observe/get/entries/len/is_empty/expire/counters`
   + tests.
3. Wire `src/lib.rs`: `pub mod state;` and crate-root re-exports. Existing
   modules and `main.rs` untouched.
4. Benchmark: `benchmarks/state.rs` (new `[[bench]]` target, registered
   explicitly like Unit 03's) with `criterion` — two cases: (a) sequential
   `observe` of 100 raw-event outputs, (b) `expire` scan over a fully
   populated store. Synthetic outputs, no I/O. Record numbers and
   conditions in the progress tracker; no performance claims beyond them.
5. Required test coverage (in-module, `#[cfg(test)]`):
   * **Write path**: raw events create entries for every payload kind
     (numeric cannot reach this path; text/boolean/structured do) with
     correct `updated_at_ms`; aggregated summaries store the mean with
     `window_end_ms`; re-observation sets `previous_value`, increments
     `updates`, refreshes recency.
   * **Read path**: `get` hit/miss; `entries()` sorted by
     `(source, kind)`; `len`/`is_empty`.
   * **Expiration**: entry past the limit is removed, returned, and
     counted; fresh entry survives; boundary case
     (`age == expire_after_ms`) expires; `None` disables; negative age
     never expires; a re-observed expired key starts fresh
     (`updates = 1`, `previous_value = None`).
   * **Capacity**: inserting beyond `max_entries` evicts the stalest entry,
     increments `evicted`, and the recent entry survives.
   * **Config**: `max_entries = 0`, `expire_after_ms = Some(0)`,
     `Some(-1)` rejected with the right variants; `Default` values assert.
   * **End-to-end within the unit**: a realistic output sequence (events +
     a closed-window summary) produces the expected readable state.

## Dependencies

None added. Standard library covers the store (`HashMap`, `Vec`).
`serde` is deliberately **not** applied to state types — no consumer
serializes runtime state yet; the optional persistence adapter adds derives
when that unit justifies it (dependency rule; consistent with spec 03's
treatment of `Aggregated`/`Output`).

Out of scope: rules (Unit 05), actions (Unit 06), runtime wiring and the
expiration scheduler (Unit 06+), persistence adapters (11+), metrics
interface (Unit 10), async runtime (decision 004).

## Verify When Done

* [ ] `cargo check` passes with no warnings.
* [ ] `cargo build` succeeds.
* [ ] `cargo test` passes — all new state tests green, all prior tests
      (foundation, event model, processing) still green.
* [ ] `cargo run` still prints `deltu 0.1.0`.
* [ ] `cargo fmt --check` passes.
* [ ] `cargo clippy` passes with no warnings (including `--all-targets`).
* [ ] `cargo bench` runs both state cases; numbers recorded in
      `context/progress-tracker.md` with hardware/conditions.
* [ ] No dependency added; `Cargo.lock` updated only for the bench target.
* [ ] Capacity and expiration paths are covered by tests (no unbounded
      growth possible).
* [ ] No rules/actions/persistence/runtime/IO code introduced (scope guard).
* [ ] `context/progress-tracker.md` updated (Unit 04 results + notes).
