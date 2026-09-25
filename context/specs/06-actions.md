# Spec 06 — Actions

Status: COMPLETE (implemented & verified 2026-09-25; finalized semantics below) · Depends on: Unit 05 (Rules)

## Goal

Implement the action layer that executes what rules request: the action
contract, a dispatcher with failure isolation (invariant 8: a failed
external action never crashes the runtime), and the structured **log**
action. Network transports (webhook, MQTT publish) are *defined by this
unit's contract but land with the async runtime in Units 07/08* — a
deliberate, documented refinement of the build order required by decision
004 (no async runtime before continuous processing needs it) and the
dependency rule (no HTTP client crate before tokio exists).

## Design

### Module

`core/actions/` maps to `src/actions/`:

```text
src/actions/
├── mod.rs        # ActionDispatcher, ActionDefinition, ActionOutcome, re-exports
├── log_action.rs # LogAction — structured log line
└── error.rs      # ActionError
```

### Contract

```rust
pub enum ActionDefinition {
    Log { level: LogLevel, template: Option<String> },
    // Webhook { url, headers, timeout_ms } — executable from Unit 07
    // MqttPublish { topic, qos } — executable from Unit 08
}

pub struct ActionRequest {           // produced by Unit 05
    pub rule_id: String,
    pub action_id: String,
    pub payload: serde_json::Value,  // rule snapshot: meaningful, compressed
}

pub struct ActionOutcome {
    pub action_id: String,
    pub rule_id: String,
    pub result: Result<Duration-in-ms + summary, ActionError>,
}
```

* `ActionDispatcher::dispatch(&mut self, requests: Vec<ActionRequest>) ->
  Vec<ActionOutcome>` — executes each action **to completion, collects
  every result, never propagates a failure** to the caller's pipeline.
* Outcomes are counted (`actions_attempted`, `actions_failed`,
  `actions_succeeded` u64s) so failures are observable (code standards:
  do not hide failures; do not fake success).
* `LogAction` writes a structured JSON line (serde_json — existing dep) with
  rule id, payload, and timestamp injected by the caller (clock-free like
  Units 03/04; the runtime owns real time). No logging framework yet —
  `tracing` arrives with the runtime in Unit 07 and LogAction migrates to it
  there.
* Per-action timeouts and retries are runtime concerns (Unit 07+); this unit
  only guarantees isolation and honest outcomes.
* An unknown `action_id` in a request is an `ActionError::UnknownAction`
  outcome — counted, never a panic.

### Config

Action definitions are validated at construction
(`ActionDispatcher::new(defs) -> Result<Self, ActionConfigError>`): empty
ids, duplicate ids, unknown log levels. Same error conventions as prior
units.

## Implementation

1. `error.rs`, `log_action.rs`, `mod.rs`; wire `src/lib.rs`.
2. Tests: log action output shape (rule id + payload present), dispatcher
   success/failure collection (use a deliberately-failing test action),
   unknown action id handling, counters, config error variants, ordering of
   outcomes matches request order.
3. No benchmark: dispatch overhead is log-formatting-bound and
   not performance-sensitive at this stage (justified deviation from the
   per-unit bench pattern; revisit in Unit 10 when metrics land).
4. Scope guard: no HTTP/MQTT transports, no retries/timeouts, no async, no
   logging framework migration.

## Dependencies

None added.

## Finalized at Unit Start (review pass, 2026-09-25)

The pre-draft left four points open; resolved here before implementation:

1. **Executor abstraction:** the dispatcher executes `Box<dyn ActionExecutor>`
   instances (`execute(&request, now_ms) -> Result<ActionSummary,
   ActionError>`) built from definitions. The spec's own failure-isolation
   test requires a "deliberately-failing test action", which is only
   possible with a pluggable executor — and Units 07/08 add webhook/MQTT
   executors behind the same seam without changing dispatch semantics.
2. **Signature refinement:** `dispatch(requests, now_ms)` — the pre-draft
   required caller-injected timestamps (clock-free) but omitted the
   parameter. `elapsed_ms` inside outcomes uses a monotonic `Instant`:
   observability, not a decision input, so the determinism principle is
   unaffected.
3. **Log sink:** the structured JSON line goes to **stderr** (the action's
   purpose; the scope guard's "no I/O" means no network/filesystem) and is
   also returned in the outcome summary so tests assert shape directly.
   Optional `template` becomes the line's `message` field with
   `{rule_id}`/`{payload}` placeholders substituted (plain replacement, no
   format runtime).
4. **`ActionRequest` stays defined in Unit 05** (single definition, already
   crate-exported); the actions module imports it rather than duplicating
   the type.

## Verify When Done

* [ ] cargo check/build/test/run, fmt --check, clippy --all-targets clean.
* [ ] Failure-isolation tests prove a failing action cannot panic or abort
      dispatch (invariant 8 pattern, tested).
* [ ] All prior tests still green; no new dependencies.
* [ ] Tracker updated (results + notes).
