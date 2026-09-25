# Topic — Rules

## Question

How should Deltu evaluate rules so behavior is deterministic, predictable,
independently testable, and free of AI for conditions a normal rule can express?

## Findings

* **Deterministic evaluation**: given the same event and the same state, a
  rule must always produce the same result. This is what makes rules
  debuggable, testable, and preferable to AI for well-defined conditions.
* **Conditions and operators**: a small, closed set of operators over typed
  fields (numeric comparison, equality, string/boolean match, existence,
  time/latency conditions) covers the practical rule space. Operators take
  typed inputs — not free-form strings evaluated at runtime.
* **Rule composition**: simple boolean combinators (all/any/not) over
  conditions express most real rules without a scripting language. A rule
  language (expressions, user-supplied scripts) is a large surface with
  injection and safety concerns — deferred until a concrete requirement.
* **Event-driven rules**: triggered by the arrival of an event
  (e.g., `temperature > 80`). State-driven rules: triggered by state
  content or transitions (e.g., "sensor X offline for 5 minutes"), which
  require either periodic evaluation or transition hooks.
* **Actions and edge cases**: a rule result maps to a configured action.
  Repeated triggering must be controlled (hysteresis or once-per-window
  semantics) so a continuously-violating condition does not fire actions
  on every event.
* **No AI here**: `temperature > 80` must remain a deterministic rule
  (code-standards: rule engine standards). AI is reserved for problems
  deterministic rules cannot solve (see `ai.md`).

## Sources

* Code standards — rule engine standards (`context/code-standards.md`)
* Architecture — rule engine ownership (`context/architecture.md`)
* Common rule-engine design references: OpenRASP/CEL-style closed operator
  sets; Google Common Expression Language (as a contrast for expression
  languages) — https://github.com/google/cel-spec

## Impact on Deltu

* The rule engine (Unit 05) defines a typed condition model with a closed
  operator set, boolean combinators, and explicit rule-to-action mapping in
  its spec; evaluation is a pure function of (event, state, rule).
* Rules are evaluated synchronously inside the pipeline — no async, no
  external calls — keeping evaluation O(1) and predictable.
* Trigger-suppression semantics (hysteresis / once-per-window) are part of
  the rule unit's scope so actions are not flooded.

## Decision

Deltu implements deterministic rules as typed conditions with a closed
operator set and boolean combinators, evaluated as pure functions against
events and state, with explicit trigger-suppression semantics. No scripting
language and no AI inside rule evaluation.
