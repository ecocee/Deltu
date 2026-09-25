# Spec 03 — Processing Core

Status: COMPLETE (implemented & verified 2026-09-25) · Depends on: Unit 02 (Event Model)

## Goal

Implement Deltu's four deterministic reduction stages — **filter,
deduplication, aggregation, change detection** — and their composition into
one synchronous pipeline over the Unit 02 event model. This unit turns raw
event streams into the meaningful outputs that the state engine (Unit 04)
and rules (Unit 05) will consume. It does **not** implement the state
engine, rules, actions, queues/backpressure, metrics infrastructure, or any
I/O.

## Design

### Ownership and module boundary

`core/processing/` in the architecture maps to the `processing` module of
the `deltu` crate. Pipeline order is the researched flow
(`research/event-processing.md`):

```text
Event → Filter → Deduplication → Aggregation → Change Detection → Output
```

Layout:

```text
src/processing/
├── mod.rs       # PipelineConfig, PipelineConfigError, Output, ProcessingPipeline, re-exports
├── filter.rs    # KindFilter — drop events outside configured kinds
├── dedup.rs     # Deduplicator — bounded id cache
├── aggregate.rs # WindowAggregator — tumbling event-time windows
└── change.rs    # ChangeDetector — deadband on aggregated values
```

Public API stays small: `ProcessingPipeline`, `PipelineConfig`,
`PipelineConfigError`, `Output`, `Aggregated` are re-exported at the crate
root; the individual stage structs remain `pub` inside `processing` for
direct testing but are not part of the promised surface.

### Stage semantics

All stages are synchronous, clock-free, and deterministic: every decision is
a pure function of the event sequence and stage configuration. Event-time
(meaning `Event.timestamp`, validated positive by Unit 02) drives windows —
no `std::time::SystemTime` anywhere in this module. The pipeline assumes
events are already validated (invariant 9 holds at the adapter boundary);
it does not re-validate.

**1. `KindFilter`** — the cheapest, most selective stage runs first.

* `allowed: Option<Vec<String>>` — `None` allows everything (the default
  must not silently drop data).
* An entry matches when `kind == entry` or `kind` starts with `entry + "."`
  (exact kind or its dotted family). `"temperature"` therefore allows
  `temperature.reading` but not `temperatures.x`.
* Entries are trimmed and lowercased at construction; empty entries are
  rejected.

**2. `Deduplicator`** — bounded, capacity-limited id cache.

* Duplicate = `Event.id` already present (spec 02: id is the producer-supplied
  dedup key). Duplicates are dropped.
* Eviction: FIFO by first insertion; when capacity is exceeded the oldest id
  is forgotten. After eviction, a replayed id is treated as new — this is the
  explicit, documented trade-off of bounded memory (research: "TTL- or
  size-limited caches"; TTL adds clock-skew concerns and is deferred).
* Stores ids only, never events.

**3. `WindowAggregator`** — tumbling event-time windows keyed by
`(source, kind)`, numeric payloads only.

* Window index = `timestamp / window_ms` (both positive, so integer
  truncation equals floor). A timestamp exactly on a boundary belongs to the
  newer window.
* Per window accumulate: `count`, `min`, `max`, `sum`, `last`. Accumulation
  order per key follows arrival order (per-source ordering; research).
* Close-on-advance: when an event arrives with a window index greater than
  the key's current open window, that window closes and emits an
  `Aggregated` summary.
* Late events (index ≤ the key's last closed index) are dropped and counted —
  the explicit late policy for the MVP.
* `max_open_windows` bounds memory: admitting a new window beyond the cap
  evicts the globally oldest open window (smallest index), counted. The
  evicted window is dropped, not emitted — it never closed.
* Non-numeric payloads pass through unchanged (aggregation is undefined for
  them; booleans/text are already meaningful as-is).

```rust
pub struct Aggregated {
    pub source: String,
    pub kind: String,
    pub window_start_ms: i64, // index * window_ms
    pub window_end_ms: i64,   // (index + 1) * window_ms
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub sum: f64,
    pub mean: f64, // sum / count
    pub last: f64,
}
```

**4. `ChangeDetector`** — deadband on the aggregated stream (research flow:
aggregation reduces first, change detection then decides what is meaningful).

* Keyed by `(source, kind)`: stores the last emitted `mean` per key.
* First summary for a key is always emitted. Later summaries are emitted
  only when `|new_mean − last_emitted| ≥ min_delta`; otherwise suppressed
  and counted.
* `min_delta: f64` must be finite and `≥ 0.0` (0.0 = never suppress — the
  honest default; suppression only happens when configured).
* State capacity bounded with FIFO eviction (same pattern as the
  deduplicator), counted.
* Raw non-numeric events bypass this stage entirely (they were emitted
  directly by the aggregator).

### Pipeline composition

```rust
pub enum Output {
    Event(Event),        // non-numeric event that survived filter + dedup
    Aggregated(Aggregated), // closed-window summary that passed the deadband
}

pub struct ProcessingPipeline { /* config + 4 stages + counters */ }

impl ProcessingPipeline {
    pub fn new(config: PipelineConfig) -> Result<Self, PipelineConfigError>;
    pub fn process(&mut self, event: Event) -> Vec<Output>;
    pub fn counters(&self) -> &PipelineCounters;
}
```

* One event yields zero, one, or more outputs (a passing numeric event can
  close a window and emit; a passing non-numeric event emits itself; drops
  emit nothing). `Vec<Output>` is therefore the honest return type.
* `PipelineCounters` — plain `u64` fields, incremented only on real events:
  `filtered_out`, `duplicates`, `late_dropped`, `windows_evicted`,
  `change_states_evicted`, `changes_suppressed`. Drops must be observable
  from day one (research: backpressure/overflow policies are only real if
  counted); formal metrics come in Unit 10.

```rust
pub struct PipelineConfig {
    pub allowed_kinds: Option<Vec<String>>, // None = allow all
    pub dedup_capacity: usize,              // default 10_000
    pub window_ms: i64,                     // default 60_000
    pub max_open_windows: usize,            // default 1_000
    pub min_delta: f64,                     // default 0.0
    pub change_state_capacity: usize,       // default 1_000
}
```

Defaults are conservative for edge-class devices (research: edge.md) and are
the documented starting point, not performance claims. `Default` derives
these values.

### Errors

`PipelineConfigError` (crate-root re-export), one variant per invalid
configuration, actionable `Display`, `impl std::error::Error`:

```rust
pub enum PipelineConfigError {
    InvalidWindowLength(i64),                              // must be > 0
    InvalidCapacity { field: &'static str, value: usize }, // must be > 0
    InvalidMinDelta(f64),                                  // must be finite and >= 0.0
    EmptyKindEntry,                                        // blank allow-list entry
}
```

All validation happens once, in stage constructors and
`ProcessingPipeline::new`. No `unwrap()`/`expect()` in production paths.

### Bounded-state invariants (tested, not aspirational)

Every piece of retained state has a configured cap and a tested eviction
path: dedup ids (`dedup_capacity`), open windows (`max_open_windows`),
change state (`change_state_capacity`). No stage can grow without bound
regardless of input volume — this is the unit's core correctness property.

## Implementation

1. `src/processing/mod.rs` — `PipelineConfig` (+ `Default`),
   `PipelineConfigError`, `Output`, `PipelineCounters`,
   `ProcessingPipeline::new/process/counters`, re-exports.
2. `src/processing/filter.rs` — `KindFilter` + tests.
3. `src/processing/dedup.rs` — `Deduplicator` + tests.
4. `src/processing/aggregate.rs` — `WindowAggregator` + `Aggregated` + tests.
5. `src/processing/change.rs` — `ChangeDetector` + tests.
6. Wire `src/lib.rs`: `pub mod processing;` and crate-root re-exports
   (`pub use processing::{...}`). `deltu::version()` and `main.rs` untouched.
7. Required test coverage (in-module, `#[cfg(test)]`):
   * **Filter**: allow-all when `None`; exact-kind match; family-prefix
     match; sibling-prefix non-match (`temperature` does not allow
     `temperatures.x`); disallowed kind dropped.
   * **Dedup**: first id passes; immediate replay dropped; oldest id evicted
     at capacity and a replay after eviction passes again; capacity 1 edge.
   * **Aggregate**: non-numeric passthrough; accumulation correctness
     (count/min/max/sum/last); close-on-advance emits with correct
     `window_start_ms`/`window_end_ms`; boundary timestamp belongs to the
     newer window; late event dropped and counted; `max_open_windows`
     eviction of the globally oldest window and counting.
   * **Change**: first summary per key emitted; below-delta suppressed and
     counted; delta ≥ `min_delta` emitted; per-key independence; state
     eviction at capacity.
   * **Pipeline (integration, in-module)**: filtered event → empty output +
     counter; duplicate → empty output + counter; numeric sequence crossing
     a window boundary → one `Aggregated` output with correct fields;
     non-numeric sequence → `Output::Event` passthrough; counter totals
     match dropped inputs.
8. Benchmark (per `research/performance.md`: stages must be measured from
   the day they exist): `benchmarks/processing.rs` with `criterion` — one
   group, three cases: filter+dedup pass-through, window aggregation over a
   boundary, full pipeline. Synthetic events, no I/O. Record the numbers and
   conditions in the progress tracker; no performance claims beyond them.
   (Implementation note: the file lives at `benchmarks/processing.rs` per
   the architecture's `benchmarks/` boundary and is registered explicitly
   in `Cargo.toml` with `harness = false`, because Cargo autodiscovers
   only the `benches/` directory.)

## Dependencies

| Crate        | Kind         | Why required | Why now | Cost / risk |
| ------------ | ------------ | ------------ | ------- | ----------- |
| `criterion`  | dev-dependency | Benchmarks for new processing stages are required by `research/performance.md` and the build plan DoD | Stages land in this unit; measuring later would benchmark a moving target | Dev-only (not in release binary); pure Rust; ARM64-safe; pinned in `Cargo.lock` |

Standard library covers everything else (`HashMap`, `VecDeque`, `f64`);
`serde` is deliberately **not** applied to `Aggregated`/`Output` — no
consumer serializes pipeline outputs yet; derivation is added by the unit
that first requires it (dependency rule: every dependency has a reason).

Out of scope: async runtime (decision 004), bounded channels/backpressure
(arrive with the runtime), state engine (Unit 04), rules (Unit 05), metrics
interface (Unit 10), HTTP/MQTT (Units 07/08).

## Verify When Done

* [ ] `cargo check` passes with no warnings.
* [ ] `cargo build` succeeds.
* [ ] `cargo test` passes — all new stage/pipeline tests green, all
      prior tests (foundation, event model) still green.
* [ ] `cargo run` still prints `deltu 0.1.0`.
* [ ] `cargo fmt --check` passes.
* [ ] `cargo clippy` passes with no warnings.
* [ ] `cargo bench` runs the three processing cases; numbers recorded in
      `context/progress-tracker.md` with hardware/conditions.
* [ ] Only dependency added: `criterion` as a dev-dependency, justified
      above; `Cargo.lock` pinned.
* [ ] Bounded-state eviction paths are covered by tests (no unbounded
      growth possible in any stage).
* [ ] No state-engine/rules/actions/queue/I-O code introduced (scope guard).
* [ ] `context/progress-tracker.md` updated (Unit 03 results + notes).
