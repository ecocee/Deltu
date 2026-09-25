# Code Standards

## General

* Keep modules small, focused, and single-purpose.
* Fix root causes instead of adding layered workarounds.
* Do not mix unrelated concerns in one module, function, class, or component.
* Prefer simple implementations over unnecessary abstractions.
* Keep the core engine independent from application-specific business logic.
* Avoid premature optimization, but benchmark performance-sensitive code.
* Avoid premature abstraction when the requirement is not yet clear.
* Prefer explicit behavior over hidden magic.
* Keep public APIs small, stable, and documented.
* Do not introduce a dependency when existing code can reasonably solve the problem.
* Do not add infrastructure merely because it is common in other projects.
* Preserve self-hosting capability.
* Prefer deterministic behavior wherever AI is not required.
* Never silently introduce cloud dependencies.
* Do not hide failures behind fallback behavior that makes the system appear successful.

---

# Rust

Rust is the primary language for the Deltu core engine.

### General Rust Rules

* Use stable Rust unless a specific feature requires otherwise.
* Run `cargo fmt` before committing Rust changes.
* Run `cargo clippy` and address relevant warnings.
* Use `cargo check` during development.
* Use `cargo test` for behavior verification.
* Prefer ownership and borrowing over unnecessary cloning.
* Avoid unnecessary heap allocations.
* Avoid unnecessary string conversions.
* Prefer strongly typed structures over loosely typed data.
* Use enums for finite states and variants.
* Use `Result` for recoverable errors.
* Do not use `unwrap()` or `expect()` in production paths unless the invariant is guaranteed and clearly justified.
* Do not silently discard errors.
* Keep error messages actionable.
* Avoid global mutable state.
* Prefer explicit dependency injection where dependencies are required.
* Keep concurrency controlled and observable.
* Avoid blocking operations inside asynchronous processing paths unless explicitly intended.
* Do not create unbounded queues.
* Use bounded buffers and explicit backpressure where appropriate.

### Rust Module Rules

Core modules should have clear responsibilities.

Preferred separation:

```text
core/
├── event/
├── input/
├── processing/
├── state/
├── rules/
├── actions/
├── ai/
├── runtime/
└── metrics/
```

Do not create a single large module containing the complete processing engine.

Do not duplicate event-processing logic across input adapters.

---

# Event Model Standards

Events are a fundamental Deltu abstraction.

Event structures should be:

* Explicit
* Serializable
* Validatable
* Stable
* Minimal

External input must be validated and normalized before entering the core engine.

Do not allow every adapter to create its own incompatible internal event representation.

Preferred flow:

```text
External Input
      ↓
Adapter
      ↓
Validation
      ↓
Normalization
      ↓
Internal Event
      ↓
Processing Engine
```

Avoid placing application-specific fields into the core event model unless there is a clear general-purpose reason.

---

# State Standards

State management must remain separate from historical persistence.

Runtime state may contain:

* Current values
* Current status
* Counters
* Aggregation windows
* Transitions
* Rule context
* Short-lived processing information

Do not treat the runtime state engine as a database.

Avoid storing unlimited historical information in memory.

State should have clearly defined lifecycle and ownership.

---

# Rule Engine Standards

Rules should be deterministic whenever possible.

Rules should:

* Have explicit inputs
* Have explicit conditions
* Produce predictable results
* Be independently testable
* Avoid unnecessary AI calls

Do not use an LLM for a condition that can be expressed reliably as a normal rule.

For example:

```text
temperature > 80
```

should remain a deterministic rule rather than an AI request.

---

# AI Standards

AI is an optional processing layer.

Never make AI a hidden dependency of normal event processing.

Preferred:

```text
Event
 ↓
Filter
 ↓
Aggregation
 ↓
State
 ↓
Rule
 ↓
AI only when required
```

Avoid:

```text
Every Event
 ↓
LLM
```

AI integrations must be isolated behind clear interfaces.

Changing from one AI provider to another should not require rewriting the event-processing engine.

AI-related code must track failures and, where applicable:

* Latency
* Token usage
* Number of calls
* Calls avoided
* Model used
* Local vs cloud execution

---

# Python

Python is primarily used for:

* AI/ML development
* Model experimentation
* Evaluation
* Training
* Research
* Python SDK

Python code must not become a second implementation of the Rust processing engine.

The Python SDK should communicate with the Deltu API rather than duplicate:

* State processing
* Rule evaluation
* Aggregation
* Event processing

Use explicit types where practical.

Validate external input.

Keep AI experiments separate from production runtime code.

---

# TypeScript / JavaScript

TypeScript is used for:

* TypeScript/JavaScript SDK
* API clients
* Optional dashboard
* Developer tooling

### Rules

* Use TypeScript rather than JavaScript for new code.
* Enable strict TypeScript settings.
* Avoid `any`.
* Prefer explicit interfaces and types.
* Use `unknown` for genuinely unknown external values.
* Narrow and validate `unknown` before use.
* Keep SDK types synchronized with the public API.
* Do not duplicate Rust engine behavior in TypeScript.
* Keep frontend code separate from core processing logic.

Example:

```typescript
function handleResponse(value: unknown): Result {
  // Validate and narrow before using value.
}
```

---

# Framework

Deltu does not require a specific frontend or application framework.

Next.js, React, Vue, Svelte, or another framework may be used for an optional dashboard or application integration.

Framework code must never become a dependency of the Rust core.

If a dashboard exists:

* Keep it behind the public API.
* Do not place event-processing logic in UI components.
* Do not make the dashboard required for engine operation.
* Keep API communication separate from presentation components.
* Keep framework-specific code inside the dashboard boundary.

---

# SDK Standards

SDKs are integration layers.

The SDK should make common operations simple:

```text
connect
emit event
query state
create rule
listen for result
trigger action
```

Avoid exposing unnecessary internal implementation details.

SDKs must:

* Use documented APIs.
* Provide predictable errors.
* Provide useful types.
* Include minimal examples.
* Remain lightweight.
* Avoid unnecessary dependencies.

SDKs must not require:

```text
PostgreSQL
Redis
Clerk
Next.js
Docker
Kubernetes
Cloud AI
```

to function.

---

# API Standards

The HTTP API is a universal integration boundary.

API handlers should:

1. Receive the request.
2. Validate input.
3. Normalize input if necessary.
4. Call the appropriate core interface.
5. Return a predictable response.
6. Report errors clearly.

Do not put long-running processing directly inside request handlers.

Do not duplicate core processing logic inside API routes.

API responses should use consistent structures.

Example:

```json
{
  "success": true,
  "data": {}
}
```

and errors should contain enough information for the client to understand what failed without exposing sensitive internal details.

Public API changes must be documented and tested.

---

# MQTT Standards

MQTT is an input/output integration, not the processing engine.

MQTT adapters must:

* Validate incoming messages.
* Convert messages into the Deltu event model.
* Handle connection failures.
* Handle reconnection where appropriate.
* Respect backpressure.
* Avoid blocking the core engine.
* Report adapter failures through logs and metrics.

MQTT-specific behavior must remain inside the MQTT adapter.

---

# Storage Standards

The core engine should prefer in-memory processing.

### Database

PostgreSQL is optional.

Use a database when there is a concrete need for:

* Historical data
* Long-term analytics
* Durable application state
* Relationships
* Large datasets

Do not make database access mandatory for core event processing.

### Cache

External caching systems such as Redis are optional.

Do not use Redis for normal in-process state or buffering when simple memory structures are sufficient.

### Files

Use filesystem storage for:

* Configuration
* Models
* Logs
* Exports
* Local artifacts

Do not store large binary models or generated artifacts directly in normal database records unless there is a documented reason.

---

# Authentication Standards

The core engine must remain authentication-provider agnostic.

Do not import a specific authentication platform into the Rust core.

Examples of external authentication systems that must remain optional:

```text
Clerk
Auth0
Firebase Auth
Supabase Auth
Custom OAuth
Custom identity systems
```

Authentication belongs at the API/application boundary.

The core should receive already-authorized operations where appropriate.

---

# Error Handling

Errors must be explicit.

Every error should answer, where practical:

* What failed?
* Where did it fail?
* Why did it fail?
* Can the operation be retried?
* Is the failure recoverable?

Do not:

* Swallow errors
* Return fake success
* Hide failures
* Crash the entire runtime because one external action failed

External failures should be isolated whenever possible.

---

# Concurrency and Background Processing

Deltu is a continuous processing system, so concurrency must be carefully controlled.

Rules:

* Prefer bounded concurrency.
* Avoid unbounded task creation.
* Use explicit worker lifecycle management.
* Use backpressure.
* Avoid uncontrolled queues.
* Support graceful shutdown.
* Do not block asynchronous workers unnecessarily.
* Ensure shared state has clear synchronization rules.
* Measure queue growth and processing latency.

A single problematic input must not consume unlimited resources.

---

# Performance Standards

Performance is part of the product.

For performance-sensitive changes, measure before and after.

Track where relevant:

* Events/sec
* Average latency
* P95 latency
* P99 latency
* CPU
* RAM
* Queue depth
* Startup time
* Sustained throughput
* Dropped events
* Failed actions
* AI calls
* AI tokens

Do not optimize based only on assumptions.

Do not use "fast", "lightweight", or "low resource" as unsupported technical claims.

---

# Security Standards

Never commit:

* API keys
* Passwords
* Access tokens
* Private keys
* Cloud credentials
* Production secrets

Validate external input.

Avoid arbitrary command execution.

Actions that affect external systems must be explicitly configured.

Do not trust data merely because it came through an internal-looking API.

Security boundaries must remain clear between:

```text
External Input
 ↓
API / Adapter
 ↓
Validation
 ↓
Core Engine
 ↓
External Action
```

---

# Dependency Standards

Before adding a dependency, verify:

* Why it is required.
* Whether the standard library can solve the problem.
* Whether an existing dependency already provides the capability.
* Whether it can remain optional.
* Its license.
* Its maintenance status.
* Its resource impact.
* Its security implications.
* Whether it creates vendor lock-in.

Avoid dependency duplication.

Do not add a dependency merely to reduce a few lines of code.

---

# File Organization

```text
core/
```

Core Rust processing engine.

```text
core/event/
```

Event definitions, validation, normalization, and serialization.

```text
core/input/
```

HTTP, MQTT, and future input adapters.

```text
core/processing/
```

Filtering, buffering, deduplication, aggregation, and change detection.

```text
core/state/
```

Runtime state and state transitions.

```text
core/rules/
```

Deterministic rules and rule evaluation.

```text
core/actions/
```

Webhooks, HTTP, MQTT, logs, and other actions.

```text
core/ai/
```

AI provider abstraction and optional AI integrations.

```text
core/runtime/
```

Runtime lifecycle, workers, configuration, concurrency, and shutdown.

```text
core/metrics/
```

Metrics and runtime observability.

```text
sdk/python/
```

Python SDK.

```text
sdk/typescript/
```

TypeScript/JavaScript SDK.

```text
adapters/
```

Optional external integrations.

```text
ai/
```

AI/ML development and experimentation.

```text
examples/
```

Working integration examples.

```text
benchmarks/
```

Performance benchmarks.

```text
tests/
```

Integration and end-to-end tests.

```text
docs/
```

Documentation and technical specifications.

```text
docker/
```

Optional Docker deployment files.

```text
dashboard/
```

Optional web dashboard.

---

# Naming Standards

Use clear, descriptive names.

Prefer:

```text
event_processor
state_store
rule_engine
mqtt_adapter
webhook_action
ai_provider
```

Avoid vague names such as:

```text
manager
helper
utils
misc
stuff
common
```

unless the module genuinely represents a general-purpose utility.

Names should describe responsibility rather than implementation history.

---

# Comments and Documentation

Comments should explain **why**, not simply repeat what the code does.

Bad:

```text
// Increment counter
counter += 1;
```

Good:

```text
// Increment only after successful processing so failed events
// are not counted as processed.
counter += 1;
```

Public APIs, important traits, configuration options, and non-obvious architectural decisions should be documented.

Do not add large comments to obvious code.

---

# Testing Standards

Every meaningful behavior should have a test.

Prefer:

```text
Unit Test
    ↓
Integration Test
    ↓
End-to-End Test
```

where appropriate.

Tests should cover:

* Normal behavior
* Invalid input
* Boundary conditions
* Failure conditions
* Recovery
* Concurrency where relevant
* Backpressure where relevant
* Serialization
* API behavior
* SDK behavior

Do not write tests that only verify implementation details when behavior can be tested directly.

---

# Generated Files

Do not manually modify generated files unless explicitly required.

Modify the source definition and regenerate the output where applicable.

Generated files should be clearly identifiable.

---

# No Unnecessary Abstraction

Do not create:

```text
Interface
 ↓
AbstractFactory
 ↓
ProviderFactory
 ↓
Manager
 ↓
Adapter
```

when a simple function or concrete implementation is sufficient.

Introduce an abstraction when there is an actual need for:

* Multiple implementations
* Testability
* Plugin support
* Public API stability
* Provider replacement
* Clear architectural separation

---

# No Unnecessary Infrastructure

Do not make the codebase depend on:

```text
Redis
PostgreSQL
Kafka
Kubernetes
Clerk
Next.js
Cloud AI
GPU infrastructure
```

unless the requirement specifically justifies it.

Optional infrastructure belongs behind adapters.

The core should remain capable of running as a lightweight self-hosted process.

---

# Code Review Checklist

Before considering a change complete, verify:

* [ ] The change solves the requested problem.
* [ ] Scope is limited to the feature.
* [ ] Core responsibilities remain correctly separated.
* [ ] No unnecessary dependency was introduced.
* [ ] No mandatory cloud dependency was introduced.
* [ ] Self-hosting still works.
* [ ] Errors are handled explicitly.
* [ ] External input is validated.
* [ ] Tests cover the changed behavior.
* [ ] Performance-sensitive changes were measured.
* [ ] Public API changes are documented.
* [ ] Relevant context files are updated.
* [ ] No secrets were added.
* [ ] Formatting and linting pass.
* [ ] Build/check commands appropriate to the changed component pass.

---

# Final Code Principle

> **Keep Deltu small, explicit, self-hostable, and composable. The Rust engine should do the hard processing. SDKs should make integration easy. Adapters should connect external systems. Optional infrastructure should stay optional.**
