# Spec 02 — Event Model

Status: DRAFT (ready to implement) · Depends on: Unit 01 (Rust Foundation)

## Goal

Define Deltu's single internal event model — the representation every input
adapter converts into and every processing stage consumes. This unit delivers
the model, its validation, normalization, and serialization. It does **not**
implement any processing, filtering, state, or rules.

## Design

### Ownership and module boundary

`core/event/` in the architecture maps to the `event` module of the `deltu`
crate (the crate *is* the core engine; a module named `core` would collide
with the built-in `core` crate). Layout:

```text
src/
├── lib.rs          # pub mod event; (existing version() stays)
├── main.rs         # unchanged — prints "deltu <version>"
└── event/
    ├── mod.rs      # Event struct, validation, normalization, re-exports
    ├── payload.rs  # Payload enum (closed set of payload kinds)
    └── error.rs    # EventError enum with actionable Display messages
```

### The model

```rust
pub struct Event {
    pub id: String,        // non-empty, producer-supplied identity (dedup key in Unit 03)
    pub source: String,    // non-empty origin, e.g. "mqtt://broker/sensors/esp32-1"
    pub kind: String,      // non-empty dotted type, normalized to lowercase, e.g. "temperature.reading"
    pub timestamp: i64,    // Unix epoch milliseconds UTC, must be > 0
    pub payload: Payload,  // closed enum, never arbitrary untyped data
}

pub enum Payload {
    Numeric(f64),          // NaN / Infinity rejected — they break deterministic rule comparison
    Text(String),          // non-empty after trim
    Boolean(bool),
    Json(serde_json::Value), // escape hatch for structured payloads; must be valid JSON by construction
}
```

Design rationale (recorded so later units don't relitigate it):

* **Identity is producer-supplied and required.** Deduplication (Unit 03)
  keys on it. Engine-side id generation (uuid/ULID) belongs to the input
  adapter units (07/08) where the requirement actually exists — not here.
* **Closed payload enum with a JSON escape hatch** keeps common telemetry
  strongly typed while accepting structured data. No app-specific fields in
  the core model (code standards: event model standards).
* **`i64` epoch millis, no chrono** — `std::time::SystemTime` covers any
  engine-side needs; timezone-aware formatting is not an event-model concern.
* **Synchronous, no async** (decision 004).

### Validation and normalization

External input is validated before entering the pipeline (architecture
invariant 9). One entry point does both:

* `Event::new(id, source, kind, timestamp, payload) -> Result<Event, EventError>`
* `Event::validate(&self) -> Result<(), EventError>` — used after
  deserialization of external data (adapter path: deserialize → validate →
  pipeline).

Rules (all explicit, none silent):

1. `id`, `source`, `kind`: trimmed; empty after trim rejected.
2. `kind`: normalized to lowercase (mixed case is accepted, not invented).
3. `source`, `id`: trimmed only — case and format preserved.
4. `timestamp`: must be `> 0`.
5. `Payload::Numeric`: must be finite (NaN/+Inf/−Inf rejected).
6. `Payload::Text`: non-empty after trim (trimmed value stored).

### Error conventions (per spec 01)

`EventError` is a project-local enum (`EmptyId`, `EmptySource`, `EmptyKind`,
`InvalidTimestamp`, `InvalidPayload`, variants finalized in implementation),
impl `std::error::Error` + actionable `Display`, converted with `From` where
needed. No `unwrap()`/`expect()` in production paths.

### Serialization

`Event` and `Payload` derive `Serialize`/`Deserialize`. Canonical JSON shape
(documented here; the public HTTP API in Unit 07 builds on it, it does not
invent a second shape):

```json
{
  "id": "evt-000001",
  "source": "mqtt://broker.local/sensors/esp32-1",
  "kind": "temperature.reading",
  "timestamp": 1769412000123,
  "payload": { "type": "numeric", "value": 21.5 }
}
```

`Payload` uses internally tagged serde representation (`"type"` field:
`numeric` | `text` | `boolean` | `json`) so JSON is self-describing.

## Implementation

1. Add the `event` module with the three files above; re-export
   `Event`, `Payload`, `EventError` from `src/event/mod.rs` and the crate
   root (`pub use event::{...}`) — this is the minimal public API surface.
2. Implement validation/normalization exactly per the rules above.
3. Derive serde traits; implement the tagged `Payload` representation.
4. Unit tests (in-module, `#[cfg(test)]`), covering at minimum:
   * valid construction for every payload variant;
   * each validation rule rejected with the expected `EventError` variant
     (empty id/source/kind, timestamp `0`, NaN, +Inf, −Inf, blank text);
   * normalization (whitespace trimmed, `kind` lowercased, id/source case
     preserved);
   * JSON round-trip: `Event → JSON → Event` equality for all payload
     variants, and the documented JSON shape for a numeric example;
   * external-input path: `serde_json` deserialization followed by
     `validate()` succeeds for valid input and fails for a negative
     timestamp.
5. `src/main.rs` and `deltu::version()` remain untouched.

## Dependencies

Two crates added in this unit — the dependency rule (build plan §19) applied:

| Crate      | Why required | Why now | Cost / risk |
| ---------- | ------------ | ------- | ----------- |
| `serde` (derive) | Event model must be serializable (code standards); the `event` module owns serialization | Units 07 (HTTP) and 08 (MQTT) both convert external payloads through this model; without it "serializable" is untested intent | Pure Rust, no unsafe, ARM64-safe, ubiquitous maintenance; compile-time only |
| `serde_json` | Canonical JSON shape above must round-trip; both future adapters speak JSON | Same as above | Pure Rust; small; pinned via Cargo.lock |

Standard-library alternative considered and rejected: hand-rolled JSON would
reimplement a serialization stack — exactly the kind of hidden maintenance
the dependency rule exists to prevent. No async runtime, no uuid, no chrono,
no HTTP/MQTT crates in this unit.

## Verify When Done

* [ ] `cargo check` passes with no warnings.
* [ ] `cargo build` succeeds.
* [ ] `cargo test` passes — all new event-model tests green, foundation test
      still green.
* [ ] `cargo run` still prints `deltu 0.1.0`.
* [ ] `cargo fmt --check` passes.
* [ ] `cargo clippy` passes with no warnings.
* [ ] Only dependencies added: `serde` (derive) and `serde_json`; both
      justified above and pinned in `Cargo.lock`.
* [ ] No processing/filtering/state/rules code introduced (scope guard).
* [ ] `context/progress-tracker.md` updated (Unit 02 result + notes).
