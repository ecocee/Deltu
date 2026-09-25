# Deltu — Build Plan

## 1. Purpose

This is the **master build plan for Deltu**.

This file is the starting point of the project.

Before implementing the actual engine, the coding agent must first understand the complete project, establish the project structure, collect required technical knowledge, document decisions, and create the implementation specifications.

The project must **not begin by immediately writing a large amount of Rust code**.

The project begins with:

```text
Understand
    ↓
Research
    ↓
Structure
    ↓
Document
    ↓
Specify
    ↓
Initialize Rust
    ↓
Build Small
    ↓
Test
    ↓
Expand Incrementally
```

---

# 2. First Principle

> **Design the system before implementing the system.**

The coding agent must not start implementing features based only on a user prompt.

The agent must first inspect the repository and understand all available context.

The `context/` directory becomes the project's engineering knowledge base.

---

# 3. First Phase — Repository Inspection

Before creating or changing implementation code, inspect the complete repository.

Check:

* Existing files
* Existing source code
* Existing configuration
* Existing documentation
* Existing Git configuration
* Existing dependencies
* Existing build files
* Existing scripts
* Existing tests
* Existing context/specification files
* Existing README
* Existing licenses
* Existing examples

Do not assume the repository is empty.

Do not delete existing work without explicit instruction.

---

# 4. Create the Context Structure

The first major task is to establish a clean project context structure.

Initial structure:

```text
context/
├── README.md
├── project-overview.md
├── architecture.md
├── code-standards.md
├── AI-WORKFLOW.md
├── developer-context.md
├── research/
│   ├── README.md
│   ├── rust/
│   ├── event-processing/
│   ├── mqtt/
│   ├── http/
│   ├── state/
│   ├── rules/
│   ├── ai/
│   ├── edge/
│   └── performance/
├── decisions/
│   ├── README.md
│   └── ...
├── specs/
│   ├── 00-build-plan.md
│   └── ...
└── progress-tracker.md
```

The exact structure may evolve.

Do not create large numbers of empty files without purpose.

Create files when their content is actually needed.

---

# 5. Context Documentation

The agent must create and maintain the core context documents.

## `project-overview.md`

Defines:

* What Deltu is
* What problem it solves
* Core principles
* Scope
* Non-goals
* Main use cases
* MVP direction
* Long-term direction

---

## `architecture.md`

Defines:

* System architecture
* Core boundaries
* Rust responsibilities
* Input boundaries
* Processing pipeline
* State
* Rules
* Actions
* AI boundary
* Storage boundary
* SDK boundary
* CLI boundary
* Optional adapters
* Deployment model

This document is authoritative for architecture.

---

## `code-standards.md`

Defines:

* Rust conventions
* Module organization
* Error handling
* Testing
* Naming
* Dependency rules
* Performance rules
* Concurrency rules
* API conventions
* Documentation conventions

---

## `AI-WORKFLOW.md`

Defines how the coding agent must work.

It must enforce:

* Read context first
* Read the relevant spec
* Work one unit at a time
* Do not invent requirements
* Do not implement future units
* Verify before completion
* Update documentation
* Update progress
* Keep architecture consistent

---

## `developer-context.md`

Defines the developer experience.

Deltu is:

```text
CLI
API
SDK
Configuration
Logs
Metrics
```

Deltu does **not** require a graphical UI.

The CLI should remain lightweight and familiar, with an operational style similar to Docker Compose.

---

# 6. Research Before Implementation

The coding agent must research technical areas that affect the architecture before implementation decisions are finalized.

Use authoritative and technically reliable sources first.

Preferred sources include:

* Official Rust documentation
* Official Cargo documentation
* Official crate documentation
* Official protocol specifications
* Official MQTT documentation
* Official HTTP documentation
* Official ONNX Runtime documentation
* Official llama.cpp documentation
* Relevant standards
* High-quality technical documentation
* Reputable engineering references

Do not blindly copy information from random blogs.

Research should answer actual engineering questions.

---

# 7. Research Collection

Research must be collected into:

```text
context/research/
```

Research should not remain only inside chat history or temporary agent reasoning.

Each research document should contain:

```markdown
# Topic

## Question

What are we trying to understand?

## Findings

Relevant technical findings.

## Sources

Official or authoritative sources.

## Impact on Deltu

How this information affects the architecture or implementation.

## Decision

What Deltu will do based on the research.
```

Research is for informing the architecture.

It is not permission to add unnecessary technology.

---

# 8. Research Topics

The initial research should cover only topics relevant to the current architecture.

### Rust

Research:

* Cargo
* Rust project structure
* Rust modules
* Error handling
* Async Rust
* Concurrency
* Channels
* Bounded queues
* Graceful shutdown
* Testing
* Benchmarking
* Cross compilation
* ARM64
* x86_64

Cargo is the Rust package manager and build tool, and `cargo init` can initialize a package in an existing directory.

Do not introduce advanced Rust architecture until it is actually required.

---

### Event Processing

Research:

* Event models
* Stream processing
* Filtering
* Deduplication
* Aggregation
* Windowing
* Change detection
* Backpressure
* Event ordering
* Bounded processing

---

### HTTP

Research:

* HTTP server architecture
* Request validation
* JSON serialization
* Error responses
* Health endpoints
* Connection handling
* Timeouts

---

### MQTT

Research:

* MQTT protocol
* Topics
* QoS
* Retained messages
* Reconnection
* Backpressure
* Payload handling
* Rust MQTT libraries

---

### State

Research:

* In-memory state
* State transitions
* State expiration
* Runtime state vs historical persistence
* Concurrent state access

---

### Rules

Research:

* Deterministic rule evaluation
* Conditions
* Operators
* Rule composition
* Event-driven rules
* State-driven rules

---

### AI

Research only after the deterministic core is understood.

Research:

* ONNX Runtime
* Local inference
* GGUF
* llama.cpp
* Model loading
* Token usage
* Local vs cloud inference
* AI provider abstraction

AI must remain optional.

---

### Edge

Research:

* ARM64
* Raspberry Pi
* CPU constraints
* Memory constraints
* Local processing
* Offline operation
* Cross compilation

---

### Performance

Research:

* Throughput measurement
* Latency
* P95/P99
* CPU usage
* Memory usage
* Queue depth
* Benchmark methodology

---

# 9. Decision Records

Important architectural decisions should be recorded in:

```text
context/decisions/
```

Example:

```text
context/decisions/
├── 001-rust-core.md
├── 002-memory-first-state.md
├── 003-ai-optional.md
└── ...
```

Each decision should explain:

```text
Decision
Reason
Alternatives considered
Consequences
```

Do not create a decision record for trivial implementation choices.

---

# 10. Establish the Architecture

After the initial research, establish the smallest valid architecture.

The initial architecture is:

```text
Input
  ↓
Event
  ↓
Processing
  ↓
State
  ↓
Rules
  ↓
Action
```

AI is outside the mandatory path:

```text
Input
  ↓
Processing
  ↓
State / Rules
  ↓
AI only when necessary
  ↓
Action
```

---

# 11. Rust First

After the context and architecture are sufficiently established, begin implementation with Rust.

Do not begin with:

* Dashboard
* Python SDK
* TypeScript SDK
* AI
* PostgreSQL
* Redis
* Docker orchestration
* Kubernetes
* Distributed processing
* Cloud infrastructure

The first implementation focus is the **small Rust core**.

---

# 12. Initialize Rust as Small as Possible

The first Rust implementation should be minimal.

Use Cargo to initialize the Rust package.

Example:

```bash
cargo init
```

Cargo officially supports `cargo init` for creating a Cargo package in an existing directory.

The first successful state should be approximately:

```text
Deltu
│
├── Cargo.toml
├── Cargo.lock
├── src/
│   └── main.rs
└── context/
```

Do not create the complete final architecture immediately.

---

# 13. First Rust Milestone

The first Rust milestone is simply:

```text
Rust project
    ↓
Compile
    ↓
Run
    ↓
Test
```

The project must successfully support:

```bash
cargo check
cargo build
cargo test
cargo run
```

Cargo's standard workflow supports building and running Rust packages through these commands.

At this stage, do not implement the event engine.

---

# 14. Grow Rust Incrementally

After the initial Rust project works, introduce functionality in very small units.

Preferred progression:

```text
Rust Project
    ↓
Basic Library/Core
    ↓
Event Type
    ↓
Event Validation
    ↓
Processing Function
    ↓
State
    ↓
Rule
    ↓
Action
```

Only introduce asynchronous runtime infrastructure when continuous processing actually requires it.

Only introduce additional crates when their purpose is justified.

---

# 15. Do Not Over-Architect the First Version

Do not immediately create:

```text
20 crates
50 modules
Multiple services
Plugin frameworks
Distributed workers
Message brokers
Databases
Complex dependency injection
Complex runtime abstractions
```

Start with the smallest architecture that can correctly implement the next requirement.

The architecture can grow when real requirements justify it.

---

# 16. Specification-Driven Development

After the initial architecture is established, every meaningful implementation unit gets a specification.

Example:

```text
context/specs/
├── 00-build-plan.md
├── 01-rust-foundation.md
├── 02-event-model.md
├── 03-processing-core.md
├── 04-state-engine.md
├── ...
```

Each specification contains:

```text
Goal
Design
Implementation
Dependencies
Verify when done
```

The specification is the source of implementation instructions for that unit.

---

# 17. First Specification

The first actual implementation specification should be:

```text
context/specs/01-rust-foundation.md
```

Its scope should be intentionally small.

It should define:

* Rust package initialization
* Cargo configuration
* Basic source structure
* Minimal core/library boundary
* Minimal executable
* Basic error/result conventions
* Basic test
* Formatting
* Clippy
* Build verification

It should **not** implement the complete event engine.

---

# 18. Progress Tracking

Maintain:

```text
context/progress-tracker.md
```

Track:

```text
Unit
Status
Started
Completed
Tests
Notes
Known Issues
```

Use statuses such as:

```text
NOT STARTED
IN PROGRESS
BLOCKED
COMPLETE
```

Do not mark a unit complete until its verification checklist passes.

---

# 19. Dependency Rule

Every dependency must have a reason.

Before adding a crate, determine:

1. What problem does it solve?
2. Is it actually required?
3. Can Rust's standard library solve it?
4. What runtime cost does it introduce?
5. Does it affect ARM64 support?
6. Does it increase maintenance?
7. Does it introduce unnecessary coupling?

Prefer fewer dependencies.

---

# 20. Infrastructure Rule

Infrastructure must be earned by requirements.

Do not introduce:

```text
PostgreSQL
Redis
Kafka
Kubernetes
Cloud services
GPU infrastructure
Vector databases
Authentication services
```

unless a specific requirement proves that the component is necessary.

---

# 21. AI Rule

AI is not part of the first Rust milestone.

The first system must work without AI.

The development order is:

```text
Deterministic Processing
        ↓
State
        ↓
Rules
        ↓
Statistics
        ↓
ML
        ↓
Local AI
        ↓
Cloud AI
```

The exact order may evolve based on research and requirements.

---

# 22. CLI Rule

The CLI is the primary operational interface.

There is no graphical UI requirement.

The CLI should eventually support simple operations such as:

```bash
plan-c up
plan-c down
plan-c status
plan-c logs
plan-c config
```

However, CLI functionality should be implemented only when the corresponding runtime functionality exists.

Do not build a large CLI before the engine exists.

---

# 23. Testing From the Beginning

Testing starts with the first Rust implementation.

Every implementation unit should include appropriate tests.

At minimum:

```text
Unit tests
Integration tests where required
Build verification
Clippy
Formatting
```

Later:

```text
Property tests
Concurrency tests
Benchmarks
End-to-end tests
```

---

# 24. Performance From the Beginning

Deltu is designed for efficient continuous processing.

Do not wait until the end to discover that the architecture is inefficient.

Measure when meaningful:

```text
CPU
Memory
Latency
Throughput
Queue depth
Allocations where relevant
```

Do not make unsupported performance claims.

---

# 25. Documentation Synchronization

Whenever implementation changes public behavior:

1. Update the relevant specification.
2. Update architecture documentation if architecture changed.
3. Update API documentation if API changed.
4. Update examples if behavior changed.
5. Update progress tracker.

Documentation must describe the actual implementation.

Never document imaginary features.

---

# 26. Build Order Principle

The project should grow approximately in this direction:

```text
00
Planning + Research + Context
        ↓
01
Rust Foundation
        ↓
02
Event Model
        ↓
03
Processing Core
        ↓
04
State
        ↓
05
Rules
        ↓
06
Actions
        ↓
07
HTTP
        ↓
08
MQTT
        ↓
09
CLI
        ↓
10
Metrics
        ↓
11+
SDKs / Persistence / AI / Adapters / Packaging
```

The exact unit ordering can change after research.

The important rule is:

> **Core Rust first. Integrations later. AI later. Infrastructure only when justified.**

---

# 27. Definition of Ready

Before implementing a unit, confirm:

* [ ] Relevant context has been read.
* [ ] Architecture supports the unit.
* [ ] Required research has been completed.
* [ ] Technical decision is documented if significant.
* [ ] Specification exists.
* [ ] Dependencies are known.
* [ ] Verification criteria are defined.

---

# 28. Definition of Done

A unit is complete only when:

* [ ] Implementation matches the specification.
* [ ] Tests pass.
* [ ] `cargo check` passes.
* [ ] `cargo fmt --check` passes.
* [ ] `cargo clippy` passes.
* [ ] Relevant benchmarks pass where applicable.
* [ ] Documentation is synchronized.
* [ ] Progress tracker is updated.
* [ ] No unnecessary dependency was introduced.
* [ ] No architecture invariant was violated.
* [ ] Existing functionality still works.

---

# 29. Agent Operating Rule

The coding agent must never treat the latest user prompt as the complete architecture.

Before making implementation decisions:

```text
Read context
    ↓
Read architecture
    ↓
Read standards
    ↓
Read workflow
    ↓
Read build plan
    ↓
Read current specification
    ↓
Inspect existing code
    ↓
Research if required
    ↓
Implement
    ↓
Verify
    ↓
Document
    ↓
Update progress
```

If information is missing, research it or document the uncertainty.

Do not silently invent an architectural decision.

---

# 30. Final Project Philosophy

Deltu should be built as a **small system that grows carefully**.

The project does not begin with a giant framework.

It begins with:

```text
Context
Research
Architecture
Specification
Rust
Tests
```

Then grows through verified small steps.

> **Start with the smallest working Rust core. Understand the system before expanding it. Add complexity only when the requirements prove it is necessary.**

> **Process data first. Use AI only when necessary.**
