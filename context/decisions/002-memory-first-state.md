# 002 — Memory-First Runtime State, Persistence Optional

Status: Accepted · Date: 2026-09-25

## Decision

The engine maintains all runtime state (current values, counters, windows,
dedup caches, queues) in memory with bounded capacities. No database is
required for normal operation. Historical persistence is an optional adapter
added later, never a core dependency.

## Reason

The primary use case — continuous processing of device/application data with
current-state rules — does not need historical storage. A memory-first design
removes PostgreSQL/Redis from the default deployment entirely, keeps latency
predictable, and matches edge operation (offline, Raspberry Pi, small RAM).
Bounded state with explicit expiration and overflow behavior prevents
unbounded memory growth during long-running operation.

## Alternatives Considered

* **Database-backed state by default** — durability for free, but violates
  the core requirement that Deltu run standalone; adds latency and an
  operational dependency to every deployment.
* **Embedded disk store by default (e.g., SQLite)** — durable without a
  server, but still introduces filesystem coupling, fsync behavior, and
  sizing concerns that most state use cases do not need.

## Consequences

* State capacity, expiration, and overflow behavior must be explicit and
  configurable (see `research/state.md`).
* State is lost on restart unless an optional persistence adapter is
  configured; that trade-off is documented, not hidden.
* Runtime state and historical persistence remain conceptually separate
  (architecture invariant 10); a persistence adapter must not change the
  engine's in-memory processing path.
