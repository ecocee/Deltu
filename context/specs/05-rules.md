# Spec 05 — Rules

Status: DRAFT (pre-drafted on request; finalize at unit start) · Depends on: Unit 03 (Processing Core), Unit 04 (State Engine)

## Goal

Implement Deltu's deterministic rule engine: typed conditions evaluated
against pipeline outputs and current state, producing action requests. Pure,
synchronous, testable — no AI, no I/O, no async (research/rules.md; code
standards: rule engine standards). Action execution itself is Unit 06.

## Design

### Module

`core/rules/` maps to `src/rules/`:

```text
src/rules/
├── mod.rs      # RuleEngine, RuleConfigError, RuleMatch, ActionRequest, re-exports
├── definition.rs # Rule, Condition, Operator, SuppressionPolicy (serde for config)
└── error.rs    # RuleError
```

Crate-root re-exports: `RuleEngine`, `Rule`, `Condition`, `RuleMatch`,
`ActionRequest`, `RuleConfigError`.

### Definitions (config-shaped, serde-derived)

```rust
pub struct Rule {
    pub id: String,               // unique, non-empty
    pub description: Option<String>,
    pub on: Trigger,              // EventTrigger or StateTrigger
    pub condition: Condition,
    pub action: String,           // action id from config (Unit 06 resolves it)
    pub suppression: Option<OncePerWindow { window_ms: i64 }>,
}

pub enum Trigger {
    /// Fires on events of one kind family (e.g. "temperature").
    Event { kind: String },
    /// Fires when a state key changes/exists (from the state engine).
    State { source: String, kind: String },
}

pub enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    Comparison {
        field: Field,
        op: Operator,
        value: StateValue-like literal, // Numeric | Text | Boolean
    },
    Exists { field: Field },
}

pub enum Field {
    EventValue,                       // the triggering event's payload
    State { source: String, kind: String }, // current state entry value
    Aggregation { stat: Stat },       // mean|min|max|sum|count|last of the triggering summary
}

pub enum Operator { Gt, Gte, Lt, Lte, Eq, Ne }
```

Closed operator set, typed literals, no scripting language, no string
evaluation (research/rules.md). `serde` derives on definitions because they
arrive from configuration; serde is already a dependency — no new crates.

### Evaluation

```rust
pub fn evaluate(&mut self, input: EvalInput) -> Vec<RuleMatch>
```

* `EvalInput` carries `Option<&Event>` and `Option<&Aggregated>` plus
  `&StateStore` — rules read state through Unit 04's interface only.
* A rule fires when its trigger matches the input and its condition
  evaluates true. Comparison against a missing state field is `false`
  (never an error, never a panic).
* Output: `RuleMatch { rule_id, action, snapshot }` ordered by rule id —
  deterministic regardless of definition order.
* **Suppression**: `OncePerWindow { window_ms }` suppresses re-firing while
  `event_ts / window_ms` is unchanged (event-time, clock-free, same
  tumbling logic as Unit 03). Suppression keys are bounded with FIFO
  eviction, counted (spec 03/04 pattern).
* Evaluation is O(rules); rules are evaluated in id order every call. No
  optimization (indexing, incremental) until benchmarks demand it.

### Errors and config

`RuleEngine::new(rules) -> Result<Self, RuleConfigError>` — duplicate ids,
empty ids, empty condition lists (`All([])` is `true`; rejected as
config error to avoid accidental always-fire), non-positive suppression
windows, unknown-field literals. `RuleConfigError` follows the established
error conventions (Display + `std::error::Error`).

## Implementation

1. `definition.rs`, `error.rs`, `mod.rs` per above; wire `src/lib.rs`.
2. Tests: every operator (numeric/text/boolean), combinators incl. truth
   tables, missing-state behavior, trigger matching (event kind family,
   state key), suppression across window boundaries + eviction counting,
   deterministic ordering, config error variants, type-mismatch comparisons
   (`Gt` on text → false, not error), serde round-trip of rule definitions.
3. Benchmark: `benchmarks/rules.rs` — evaluate 100 rules (mixed conditions)
   against one input; registered `[[bench]]` like prior units.
4. Scope guard: no action execution, no state *writes*, no I/O, no AI.

## Dependencies

None added. `serde`/`serde_json` reused for definitions; `criterion`
dev-dependency reused for the benchmark.

## Verify When Done

* [ ] cargo check/build/test/run, fmt --check, clippy --all-targets all clean.
* [ ] All prior tests still green; new rule tests pass.
* [ ] `cargo bench` includes the rules case; numbers recorded in tracker.
* [ ] No new dependencies; no action execution or I/O (scope guard).
* [ ] Tracker updated (results + notes).
