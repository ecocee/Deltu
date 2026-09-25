# AI Workflow Rules

## Approach

Build Deltu incrementally using a **spec-driven, architecture-first workflow**.

The context files define what Deltu is, how it should be structured, what is currently in scope, and what decisions have already been made.

Before implementing any feature:

1. Read the relevant context files.
2. Understand the current architecture.
3. Confirm the feature belongs to the current scope.
4. Implement the smallest complete unit.
5. Test the unit.
6. Update documentation and progress.
7. Only then move to the next unit.

The primary product is the **Rust core engine**. SDKs, adapters, AI integrations, storage integrations, and the dashboard are supporting components.

Do not introduce technologies simply because they are common in modern web applications.

Do not assume that Deltu requires Next.js, PostgreSQL, Redis, Clerk, Kubernetes, or cloud infrastructure.

---

## Product Development Principles

* Build the smallest useful implementation first.
* Prefer a working feature over speculative architecture.
* Keep the core engine lightweight.
* Keep the core engine self-hostable.
* Keep external dependencies optional whenever practical.
* Prefer local processing.
* Prefer deterministic processing before AI.
* Use AI only when it provides meaningful additional value.
* Keep SDKs simple.
* Keep public APIs smaller than internal implementations.
* Measure performance instead of making unsupported performance claims.
* Do not build infrastructure before a real requirement exists.
* Do not build features merely because they may be useful later.

---

## Context File Priority

When making an implementation decision, use this order:

1. `architecture.md`
2. `project-overview.md`
3. Other feature-specific context files
4. `progress-tracker.md`
5. Existing code
6. Developer conventions
7. New architecture decisions

Existing code must not override an explicit architecture decision.

If the code conflicts with the current specification, determine whether the specification or implementation should be updated before continuing.

Never silently change the architecture to accommodate existing code.

---

## Scoping Rules

* Work on one feature unit at a time.
* Prefer small, verifiable increments.
* Keep each implementation unit independently testable.
* Do not combine unrelated system boundaries.
* Do not implement future features during an MVP task.
* Do not refactor unrelated code while implementing a feature.
* Do not add infrastructure without a concrete requirement.
* Do not add dependencies without understanding their effect on self-hosting and resource usage.

A feature unit should have:

```text
Requirement
    ↓
Implementation
    ↓
Test
    ↓
Documentation
    ↓
Progress Update
```

---

## When to Split Work

Split an implementation step if it combines:

* Rust core processing and dashboard UI changes
* SDK development and unrelated core-engine refactoring
* Multiple unrelated API routes
* Input adapters and unrelated AI functionality
* Database integration and unrelated event-processing changes
* Authentication changes and unrelated processing logic
* Multiple unrelated protocol integrations
* Core engine changes and deployment infrastructure changes
* Multiple features that cannot be tested independently
* Behavior that is not clearly defined in the context files

If a change cannot be verified end to end within a reasonable scope, the scope is too broad and must be split.

---

## Implementation Order

When building a new capability, prefer this sequence:

```text
1. Specification
2. Data model
3. Core implementation
4. Unit tests
5. Integration test
6. API/SDK integration
7. Example
8. Documentation
9. Benchmark if performance-sensitive
10. Progress update
```

Do not start with UI or infrastructure when the underlying core behavior is not defined.

---

## Rust Core Rules

The Rust core is the primary product.

Rust code should own:

* Event processing
* Validation
* Normalization
* Buffering
* Filtering
* Deduplication
* Aggregation
* Change detection
* State management
* Rule evaluation
* Scheduling
* Action execution
* Runtime lifecycle
* Concurrency
* Backpressure
* Metrics
* Core error handling

The Rust core must remain independent from:

* Next.js
* Clerk
* PostgreSQL
* Redis
* Cloud AI
* A specific dashboard
* A specific customer's business logic

Avoid unnecessary Rust dependencies.

Prefer simple data structures and predictable resource usage.

Do not introduce a framework when a small internal implementation is sufficient.

---

## SDK Rules

The SDK exists to make Deltu easy to integrate into existing applications.

Supported SDKs may include:

* Python
* TypeScript / JavaScript

SDKs must:

* Provide simple developer interfaces.
* Communicate with Deltu through documented APIs.
* Hide unnecessary protocol details.
* Follow the public API specification.
* Provide useful error handling.
* Include examples and tests.

SDKs must not:

* Reimplement the Rust processing engine.
* Maintain a second rules engine.
* Maintain a second state engine.
* Add mandatory infrastructure.
* Require users to adopt a particular web framework.

The goal is:

```text
Existing Application
        ↓
Deltu SDK
        ↓
Deltu Engine
```

not:

```text
Existing Application
        ↓
Replace existing architecture
        ↓
Adopt Deltu's entire stack
```

---

## API Rules

The API must remain language-agnostic.

HTTP/JSON should provide a simple integration path for applications that do not use an official SDK.

Public API behavior must be:

* Explicit
* Documented
* Versioned when necessary
* Testable
* Backward-compatible where promised

Never invent an API response or request structure without documenting it.

Avoid exposing internal implementation details unnecessarily.

---

## Data Processing Rules

Deltu processes data continuously.

The preferred processing flow is:

```text
Input
 ↓
Validation
 ↓
Normalization
 ↓
Buffer
 ↓
Filtering
 ↓
Deduplication
 ↓
Aggregation
 ↓
Change Detection
 ↓
State Update
 ↓
Rule Evaluation
 ↓
AI When Needed
 ↓
Action
```

Not every input event should become:

* A database record
* An AI request
* A network request
* A permanent event

The engine should reduce unnecessary processing whenever possible.

---

## AI Rules

AI is optional.

The system must work correctly without AI.

Use this decision hierarchy:

```text
Rules
 ↓
Statistics
 ↓
Tiny ML
 ↓
Local ML
 ↓
Local GenAI
 ↓
Cloud AI
```

Use the simplest mechanism that solves the problem.

Do not call AI merely because AI is available.

AI should normally receive meaningful, processed information rather than an uncontrolled raw event stream.

Track:

* AI calls
* AI calls avoided
* Tokens used
* Estimated cost where applicable
* AI latency
* AI failures

---

## Local AI Rules

Local AI should be preferred when practical for:

* Privacy-sensitive processing
* Offline operation
* Edge deployments
* Low-latency processing
* Cost reduction

Possible local runtimes include:

* ONNX Runtime
* llama.cpp
* GGUF-compatible models

Local AI must remain optional.

The base Deltu installation must not require downloading a large AI model.

---

## Storage Rules

The core runtime should prefer in-memory processing.

Do not add PostgreSQL merely to store every incoming event.

Do not add Redis merely for buffering or caching when in-process memory is sufficient.

External storage should be introduced only when a concrete requirement exists.

Storage categories:

```text
In-Memory
→ Current runtime state and temporary processing data

Local Persistence
→ Optional durable local state

PostgreSQL
→ Optional historical data and analytics

Filesystem
→ Configuration, models, logs, exports, artifacts

External Cache
→ Optional distributed/shared state
```

---

## Self-Hosting Rules

Deltu must remain easy to self-host.

A minimal installation should be capable of running as:

```text
plan-c
```

or:

```text
Docker
 └── Deltu
```

Do not require users to install multiple infrastructure services for basic functionality.

The following must remain optional:

* PostgreSQL
* Redis
* Kubernetes
* Cloud services
* Cloud AI
* Dashboard
* Authentication providers
* GPU infrastructure

---

## Authentication Rules

Deltu must not depend on a specific authentication provider.

Do not introduce Clerk, Auth0, Firebase Auth, or another authentication SaaS into the core.

Authentication should occur at the API boundary.

Local trusted deployments may run without authentication.

Remote deployments must provide an appropriate authentication mechanism before production exposure.

Application-level users, organizations, tenants, billing, and permissions belong outside the core engine unless a future architecture decision explicitly adds them.

---

## Background Task Rules

Continuous processing must run through controlled runtime workers.

Do not perform long-running processing directly inside HTTP request handlers.

Background processing must support:

* Controlled concurrency
* Backpressure
* Graceful shutdown
* Failure isolation
* Error reporting
* Metrics

A failure in one external action must not automatically terminate the entire runtime.

---

## Dependency Rules

Before adding a dependency, answer:

1. What exact problem does it solve?
2. Can the standard library or existing code solve it?
3. Can the dependency remain optional?
4. Does it increase binary size?
5. Does it increase CPU or RAM usage?
6. Does it make self-hosting harder?
7. Does it create a cloud dependency?
8. Does it introduce security or licensing concerns?
9. Does it lock Deltu to a vendor?
10. Is the dependency necessary for the current scope?

If the answer does not justify the dependency, do not add it.

---

## Protected Files

Do not modify the following unless explicitly instructed:

* Third-party library source code
* Generated dependency files
* Generated SDK artifacts
* Generated API clients
* Lockfiles solely to make unrelated changes
* User-specific configuration files
* Deployment secrets
* `.env` files containing secrets
* External model files
* Existing architecture/context files without checking their purpose first

If generated files must change because the source specification changed, regenerate them through the documented process instead of manually editing them.

---

## Testing Rules

Every implemented behavior must have an appropriate verification method.

### Rust Core

Use:

* Unit tests
* Integration tests
* Property tests where useful
* Concurrency tests where relevant
* Failure-path tests

### API

Test:

* Valid requests
* Invalid requests
* Authentication behavior where applicable
* Error responses
* Serialization
* Integration with the core engine

### SDK

Test:

* Client creation
* Event submission
* API communication
* Error handling
* Serialization
* Compatibility with the documented API

### Adapters

Test:

* Connection
* Input conversion
* Output behavior
* Failure handling
* Reconnection where applicable

---

## Performance Testing

Performance-sensitive changes should include benchmarks when practical.

Measure:

* Events per second
* Average latency
* P95 latency
* P99 latency
* CPU usage
* Memory usage
* Queue size
* Startup time
* Sustained processing
* Dropped events
* Failed actions

AI-related features should additionally measure:

* AI calls
* Tokens
* AI latency
* AI calls avoided

Do not use vague claims such as "extremely fast" or "zero CPU."

Use measured results.

---

## Documentation Rules

Documentation is part of the implementation.

Update the relevant context file when changing:

* Architecture
* System boundaries
* Storage
* Authentication
* Public APIs
* SDK behavior
* Feature scope
* Dependencies
* AI behavior
* Deployment
* Performance assumptions

Documentation must describe actual implemented behavior.

Do not document features that do not exist.

---

## Keeping Context Files in Sync

Whenever implementation changes an architectural decision, update the relevant context file.

At minimum, keep these synchronized:

```text
project-overview.md
architecture.md
AI-WORKFLOW.md
progress-tracker.md
```

Feature-specific documentation should also be updated when applicable.

If the code and documentation disagree, stop and resolve the discrepancy.

---

## Handling Missing Requirements

Do not invent product behavior that is not defined.

If a requirement is ambiguous:

1. Identify the ambiguity.
2. Check existing context files.
3. Check related architecture decisions.
4. If still unresolved, record it as an open question.
5. Resolve the requirement before implementing behavior that depends on it.

Open questions should be tracked in:

```text
progress-tracker.md
```

Do not silently choose a complex architecture to resolve an undefined requirement.

---

## Architecture Changes

Any change that affects the following requires explicit consideration:

* Core engine boundaries
* Public API
* Event model
* State model
* Storage requirements
* AI architecture
* SDK architecture
* Deployment model
* Mandatory dependencies
* Security boundaries

Before making such a change, document:

```text
Problem
Decision
Reason
Alternatives considered
Impact
```

Do not introduce major architecture changes inside an unrelated feature implementation.

---

## No Unnecessary Infrastructure

Do not add infrastructure simply because it is common in production systems.

Do not automatically introduce:

```text
Redis
Kafka
PostgreSQL
Kubernetes
Docker Compose
Service Mesh
Message Broker
Vector Database
Cloud Queue
```

First determine whether Deltu actually requires it.

The preferred architecture is:

```text
One Rust Process
       ↓
Simple Integration
```

Additional infrastructure should be added only when a real requirement cannot reasonably be solved by the existing architecture.

---

## No Framework Lock-In

Deltu must not become dependent on a particular application framework.

Users should be able to integrate it with:

```text
Next.js
React
Vue
Angular
Svelte
Python
FastAPI
Django
Go
Java
Rust
C++
Embedded systems
Custom software
```

through APIs, SDKs, protocols, or adapters.

Deltu is an engine, not an application framework.

---

## Open-Source Rules

Deltu is intended to be open source and self-hostable.

Development should prioritize:

* Clear documentation
* Reproducible builds
* Simple installation
* Public APIs
* Contributor-friendly structure
* Automated tests
* Transparent architecture

The intended license is Apache License 2.0 unless changed by an explicit project decision.

Project ownership, contributor rights, trademarks, and contribution agreements must be handled separately from the runtime license.

Do not add proprietary dependencies that prevent normal open-source use without documenting the licensing impact.

---

## No Fake Features

Do not create:

* Fake AI responses
* Fake metrics
* Fake event processing
* Placeholder success states presented as real functionality
* Mock production behavior without clearly marking it as mock
* Unimplemented API endpoints presented as complete
* Performance claims without benchmarks

If something is not implemented, clearly identify it as incomplete.

---

## Demo-First Validation

For important capabilities, create a small working example.

Examples:

### Basic Event Processing

```text
HTTP Event
 ↓
Deltu
 ↓
Filter
 ↓
State
 ↓
Rule
 ↓
Webhook
```

### Sensor Processing

```text
ESP32
 ↓
MQTT
 ↓
Deltu
 ↓
Aggregation
 ↓
Change Detection
 ↓
Rule
 ↓
Alert
```

### Optional AI

```text
Continuous Data
 ↓
Processing
 ↓
Meaningful Event
 ↓
AI
 ↓
Structured Result
 ↓
Action
```

The demo must use the real implementation, not a simulated architecture.

---

## Before Moving to the Next Unit

Before starting the next implementation unit:

1. The current unit works within its defined scope.
2. Appropriate tests pass.
3. No invariant in `architecture.md` was violated.
4. No unnecessary dependency was introduced.
5. Relevant documentation is updated.
6. `progress-tracker.md` reflects the current state.
7. Public API changes are documented.
8. Performance-sensitive changes are benchmarked where appropriate.
9. The implementation does not silently introduce a cloud dependency.
10. The repository remains buildable.

Use the appropriate verification command for the component being changed.

For example:

```text
Rust:
cargo check
cargo test

Python:
pytest

TypeScript:
npm/pnpm/yarn test
npm/pnpm/yarn build

Docker:
docker build

Full repository:
Run the repository-defined validation commands.
```

Do **not** assume `npm run build` is the universal project verification command.

---

## Git Discipline

Keep commits focused.

Prefer:

```text
feat: add event validation
feat: add aggregation engine
feat: add state transitions
feat: add webhook action
test: add event pipeline tests
docs: update architecture
```

Avoid mixing:

```text
feature + unrelated refactor + UI redesign + dependency migration
```

in one change.

Do not rewrite unrelated files merely to make the current diff look cleaner.

---

## MVP Discipline

The first version should focus on the smallest useful continuous-processing engine.

Initial target:

```text
HTTP / MQTT
      ↓
Event
      ↓
Filter
      ↓
Aggregation
      ↓
Change Detection
      ↓
State
      ↓
Rule
      ↓
Webhook
```

AI is not required for the first functional engine.

Do not delay the core engine because future AI functionality has not been implemented.

Do not build the dashboard before the underlying processing behavior is stable enough to expose.

---

## Final Development Principle

> **Build the engine first. Keep it small. Make it self-hostable. Let developers connect their existing systems through the SDK and API. Add infrastructure only when a real requirement demands it. Process data first and use AI only when necessary.**
