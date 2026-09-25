# Topic — State

## Question

How should Deltu maintain runtime state in memory so rules and actions see
current values without a database, without unbounded growth, and without
losing the separation between runtime state and historical persistence?

## Findings

* **In-memory state**: a memory-first engine keeps current values, counters,
  and windows in ordinary data structures. This is what enables operation
  with no PostgreSQL, Redis, or any external service (architecture
  invariant 2).
* **State transitions**: storing the current value plus a timestamp (and,
  where useful, the previous value) is enough to detect transitions and feed
  change detection; full transition logs are a persistence concern, not a
  runtime-state concern.
* **State expiration**: entries for sources that stop reporting must expire,
  otherwise memory grows forever with churned device ids. Expiration is
  periodic and bounded (scan + evict), not per-event.
* **Runtime vs historical**: runtime state answers "what is true now";
  historical persistence answers "what happened before". Mixing them makes
  the engine stateful-heavy and slow. Deltu keeps them conceptually separate
  (invariant 10); historical storage is an optional adapter.
* **Concurrent access**: the safe default is a small number of synchronization
  points — e.g., a single state store guarded by a lock, or per-key sharded
  locks if contention is measured. Per-event fine-grained locking
  architectures add complexity before there is evidence of contention.
  Single-threaded pipeline processing (one event at a time through the
  stages) can eliminate most shared-state contention entirely.

## Sources

* std::sync::Mutex / RwLock documentation — https://doc.rust-lang.org/std/sync/
* std::collections::HashMap — https://doc.rust-lang.org/std/collections/hashmap/
* Kleppmann, *Designing Data-Intensive Applications* (state vs event log)

## Impact on Deltu

* The state engine (Unit 04) is a keyed store of current values with
  timestamps, bounded entries, and periodic expiration; its capacity limits
  are configured, and overflow behavior is explicit.
* State exposes a narrow read interface for rules and actions; it is not a
  general database and never persists by default.
* Concurrency design stays as simple as possible: single-pipeline processing
  first, sharding only if benchmarks show contention.

## Decision

Deltu implements a bounded, in-memory, keyed state store with timestamps,
configured capacity, and periodic expiration. Runtime state and historical
persistence remain separate; persistence appears later only as an optional
adapter behind an explicit interface.
