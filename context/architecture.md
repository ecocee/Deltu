# Architecture Context

## Architecture Philosophy

Deltu is an **open-source, self-hosted, SDK-first continuous data processing engine**.

The core product is a lightweight Rust runtime that receives continuous data, processes it locally, maintains state, evaluates rules, optionally uses AI, and triggers actions.

Deltu must not force users to adopt a specific web framework, authentication provider, database, cache, cloud provider, or frontend framework.

Users should be able to connect Deltu to their existing applications through simple APIs, SDKs, and adapters.

### Core principle

> **Bring Deltu into the user's existing stack — do not force the user's stack to become Deltu.**

The minimum Deltu installation should be capable of running as a single lightweight process without a database, cache, frontend, cloud service, or AI model.

---

# Stack

| Layer             | Technology                          | Role                                                                                                                                   |
| ----------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Core Engine       | Rust                                | Continuous event processing, buffering, filtering, deduplication, aggregation, change detection, state, rules, scheduling, and runtime |
| Runtime           | Native Rust Binary                  | Lightweight self-hosted execution with low CPU and RAM overhead                                                                        |
| HTTP              | Rust HTTP Server                    | Receive events, expose APIs, health endpoints, status, and developer integration                                                       |
| MQTT              | Rust MQTT Integration               | Receive continuous sensor and IoT events                                                                                               |
| Event Model       | Rust Structs + Serialization        | Common internal event representation and validation                                                                                    |
| Stream Processing | Rust                                | Process continuous data efficiently without requiring external streaming infrastructure                                                |
| State Engine      | Rust                                | Maintain current state, counters, windows, transitions, and runtime context in memory                                                  |
| Rule Engine       | Rust                                | Deterministic condition evaluation without requiring AI                                                                                |
| Action Engine     | Rust                                | Execute configured outputs such as webhooks, HTTP, MQTT, logs, and other adapters                                                      |
| AI Interface      | Rust Provider Abstraction           | Optional connection to AI/ML providers without coupling the engine to a specific model                                                 |
| Local ML          | ONNX Runtime / Compatible Runtime   | Optional local machine-learning inference                                                                                              |
| Local GenAI       | llama.cpp / GGUF-compatible Runtime | Optional local generative AI inference                                                                                                 |
| AI Development    | Python                              | Model experimentation, training, evaluation, and AI research                                                                           |
| Python SDK        | Python                              | Allow Python applications to connect to Deltu                                                                                         |
| TypeScript SDK    | TypeScript                          | Allow Node.js, TypeScript, and JavaScript applications to connect to Deltu                                                            |
| API               | HTTP/JSON                           | Universal integration interface for any language or application                                                                        |
| Configuration     | YAML / JSON / Environment Variables | Simple runtime configuration                                                                                                           |
| Logging           | Structured Logging                  | Runtime diagnostics, events, errors, and operational information                                                                       |
| Metrics           | Lightweight Internal Metrics        | Throughput, latency, CPU, RAM, queue size, errors, AI calls, and token usage                                                           |
| Runtime Storage   | In-Memory                           | Current state, buffers, aggregation windows, counters, and short-lived runtime information                                             |
| Local Persistence | Optional                            | Durable state or local historical information when required                                                                            |
| Database Adapter  | PostgreSQL, Optional                | Historical event storage and analytics for deployments that need a database                                                            |
| Cache Adapter     | Optional                            | External cache/shared state only when a deployment actually requires it                                                                |
| File Storage      | Local Filesystem                    | Configuration, models, exports, logs, and local artifacts                                                                              |
| Container         | Docker, Optional                    | Reproducible self-hosted deployment                                                                                                    |
| Platform          | Linux / ARM64 / x86_64              | Primary deployment platforms                                                                                                           |
| Edge              | Raspberry Pi / ARM64                | Low-resource local and edge deployment                                                                                                 |
| Dashboard         | Optional TypeScript Web Application | Optional administration, monitoring, and visualization UI                                                                              |
| MCP               | Optional Adapter                    | Allow compatible AI agents to interact with Deltu; not a core dependency                                                              |

---

# Core vs Optional Architecture

Deltu must clearly separate its **core engine** from optional integrations.

## Core

The following must work without external infrastructure:

```text
Rust Runtime
    │
    ├── Event Processing
    ├── Buffering
    ├── Filtering
    ├── Deduplication
    ├── Aggregation
    ├── Change Detection
    ├── State
    ├── Rules
    ├── Scheduling
    ├── Actions
    ├── Metrics
    └── Logging
```

## Optional

```text
SDKs
├── Python
└── TypeScript

Input Adapters
├── HTTP
├── MQTT
└── Future protocols

AI
├── ONNX
├── llama.cpp / GGUF
├── Local AI
└── Cloud AI providers

Storage
├── Local persistence
└── PostgreSQL

Infrastructure
├── Docker
└── External cache

Interfaces
├── Dashboard
└── MCP
```

Optional components must not become mandatory dependencies of the core engine unless a future architecture decision explicitly justifies the change.

---

# System Boundaries

The repository should maintain clear ownership between the processing engine, integrations, SDKs, AI components, and optional interfaces.

* `core/` — Owns the main Rust processing engine and must remain independent of application-specific business logic.

* `core/event/` — Owns event definitions, validation, normalization, serialization, and event-related types.

* `core/input/` — Owns input adapters such as HTTP and MQTT and converts external data into the internal event model.

* `core/processing/` — Owns buffering, filtering, deduplication, aggregation, change detection, and continuous stream processing.

* `core/state/` — Owns runtime state, state transitions, counters, windows, and state access.

* `core/rules/` — Owns deterministic rules, conditions, operators, rule evaluation, and rule-triggered processing.

* `core/actions/` — Owns configured actions such as webhooks, HTTP requests, MQTT messages, logs, and future output adapters.

* `core/ai/` — Owns the AI provider abstraction and optional AI integrations. AI must remain isolated from the deterministic processing engine.

* `core/runtime/` — Owns application startup, configuration, lifecycle, workers, concurrency, scheduling, and graceful shutdown.

* `core/metrics/` — Owns internal metrics such as throughput, latency, queue size, errors, resource usage, and AI usage.

* `sdk/python/` — Owns the Python SDK and developer-facing Python integration. It must not duplicate the Rust processing engine.

* `sdk/typescript/` — Owns the TypeScript/JavaScript SDK and developer-facing API client.

* `adapters/` — Owns optional integrations with external systems, databases, caches, protocols, AI providers, and other infrastructure.

* `ai/` — Owns model development, experimentation, evaluation, datasets, training, and AI-specific tooling. It must not unnecessarily become a dependency of the Rust runtime.

* `examples/` — Owns minimal working examples demonstrating how external applications connect to Deltu.

* `benchmarks/` — Owns performance and resource benchmarks.

* `tests/` — Owns integration, end-to-end, compatibility, and system-level tests.

* `docs/` — Owns technical documentation, architecture documentation, SDK documentation, deployment guides, and examples.

* `docker/` — Owns optional Docker deployment configuration.

* `dashboard/` — Owns the optional web dashboard. It must not contain core event-processing logic.

---

# Storage Model

Deltu follows a **memory-first and persistence-optional** storage model.

## In-Memory Runtime State

Used for:

* Current state
* State transitions
* Event buffers
* Aggregation windows
* Counters
* Change detection context
* Rule evaluation context
* Short-lived processing information
* Runtime queues

The core engine must not require a database for normal operation.

## Local Persistence

Optional local persistence may contain:

* Durable runtime state
* Configuration
* Local historical information
* Local event snapshots
* Recovery information
* Exported results

Persistence should only be enabled when the deployment actually requires it.

## PostgreSQL

PostgreSQL is an **optional adapter**, not a core dependency.

It may be used for:

* Historical events
* Long-term measurements
* Analytics
* Relationships
* Large historical datasets
* Application-specific persistence

Deltu must still operate without PostgreSQL.

## Cache

An external cache such as Redis is **optional**.

The core engine should use in-process memory for normal buffering and state management.

An external cache may be introduced when a deployment specifically requires:

* Shared state between multiple Deltu instances
* External caching
* Distributed coordination
* Cross-process queues
* High-scale deployment requirements

Redis must never be added simply because it is a common infrastructure component.

## File Storage

The local filesystem may contain:

* Configuration files
* ONNX models
* GGUF models
* Local AI artifacts
* Logs
* Exported data
* Backup files
* Development artifacts

Large files and AI models must not be unnecessarily stored inside the runtime state system.

---

# Auth and Access Model

Deltu is an infrastructure engine and should not force a particular authentication provider.

## Local Usage

For local/self-hosted usage:

```text
Application
    ↓
localhost
    ↓
Deltu
```

The engine may run without authentication when bound to a trusted local interface.

## Remote API

When Deltu is exposed over a network, authentication must be supported before production exposure.

The core API should support a lightweight authentication mechanism such as:

* API keys
* Access tokens
* Configured credentials

Authentication should happen at the API boundary.

## External Authentication

Deltu must not require:

* Clerk
* Auth0
* Firebase Auth
* Supabase Auth
* Any specific authentication SaaS

A user's existing application may handle authentication and then communicate with Deltu.

For example:

```text
User
 ↓
Customer Application
 ↓
Existing Authentication
 ↓
Deltu SDK/API
 ↓
Deltu Engine
```

## Ownership

Ownership and application-level authorization belong to the application or API layer.

Deltu core should not contain assumptions about:

* Users
* Organizations
* Tenants
* Billing accounts
* Subscription plans
* Application roles

A higher-level application can add these capabilities around Deltu when required.

## Access Control

Actions affecting external systems must be explicitly configured.

Deltu must not provide unrestricted arbitrary execution by default.

---

# AI and Background Processing Model

## AI Is Optional

Deltu must function completely without AI.

The normal processing hierarchy is:

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

The system should use the cheapest and simplest mechanism capable of solving the problem.

## AI Triggering

Incoming events must not automatically be sent to an AI model.

The preferred flow is:

```text
Raw Data
    ↓
Filtering
    ↓
Aggregation
    ↓
Change Detection
    ↓
State
    ↓
Rule
    ↓
Meaningful Event
    ↓
AI Only If Needed
```

AI should receive meaningful compressed information rather than unnecessary raw streams.

## Local AI

When AI is required, local inference should be preferred when practical.

Possible runtimes include:

* ONNX Runtime
* llama.cpp
* GGUF models
* Other compatible local inference runtimes

Cloud AI may be supported through optional providers.

No cloud AI provider should become a mandatory dependency.

## Background Processing

Long-running processing must happen inside the runtime's controlled worker/event-processing system.

HTTP request handlers must not perform long-running continuous processing directly.

Background workers should have:

* Controlled concurrency
* Backpressure
* Graceful shutdown
* Failure isolation
* Clear lifecycle management
* Observable metrics

---

# SDK and Integration Model

Deltu is **SDK-first**, not framework-first.

A developer should be able to keep their existing technology stack.

For example:

```text
Python Application
       │
       └── Deltu Python SDK
                    │
                    ▼
                Deltu
```

or:

```text
Next.js / Node Application
       │
       └── Deltu TypeScript SDK
                    │
                    ▼
                Deltu
```

or:

```text
Any Application
       │
       └── HTTP API
                    │
                    ▼
                Deltu
```

The developer should not need to install:

```text
Next.js
NestJS
Clerk
PostgreSQL
Redis
Kubernetes
```

just to use Deltu.

---

# Deployment Model

Deltu must support several deployment models.

## Single Binary

Preferred minimal deployment:

```text
Operating System
      │
      └── plan-c
```

No external infrastructure required.

## Docker

Optional:

```text
Docker
  └── Deltu
```

## Edge Device

Example:

```text
ESP32
  ↓
MQTT
  ↓
Raspberry Pi
  ↓
Deltu
  ↓
Local Action
```

## Server

```text
Application Server
       │
       └── Deltu
```

## Larger Deployment

Only when justified:

```text
Load Balancer
      ↓
Deltu Instance
      ↓
Optional Shared Storage / Database / Cache
```

Distributed infrastructure must be introduced only when actual scale requirements justify it.

---

# Dashboard Boundary

The dashboard is optional.

Deltu must remain fully functional without a dashboard.

The dashboard may provide:

* Runtime status
* Event monitoring
* State visualization
* Rule management
* Configuration
* Metrics
* AI usage
* Logs
* Health information

The dashboard must communicate with Deltu through its public API.

The dashboard must not implement duplicate event-processing logic.

---

# Performance and Resource Model

Deltu is designed for efficient continuous processing.

The primary resource goals are:

* Low CPU usage
* Low RAM usage
* Low network usage
* Minimal unnecessary storage
* Minimal GPU usage
* Minimal AI calls
* Minimal AI tokens
* Fast startup
* Predictable latency
* Stable long-running operation

The project must measure performance instead of making unsupported claims.

Important measurements include:

* Events per second
* Average latency
* P95 latency
* P99 latency
* CPU usage
* Memory usage
* Queue size
* Startup time
* Sustained throughput
* Dropped events
* Failed actions
* AI calls
* AI tokens
* AI calls avoided through deterministic processing

---

# Invariants

The following rules must never be violated without an explicit architecture decision.

1. **The core engine must work without AI.**

2. **The core engine must work without PostgreSQL, Redis, Clerk, Next.js, or any other external application infrastructure.**

3. **Deltu must remain self-hostable and must not silently depend on a cloud service.**

4. **The Rust engine owns continuous event processing; SDKs and dashboards must not duplicate core processing logic.**

5. **Incoming data must not automatically be sent to AI.**

6. **Deterministic processing must be preferred over AI when deterministic processing is sufficient.**

7. **HTTP request handlers must not perform uncontrolled long-running background work.**

8. **A failed external action must not crash the entire Deltu runtime.**

9. **External input must be validated before entering the core processing pipeline.**

10. **Runtime state and historical persistence must remain conceptually separate.**

11. **The core engine must not require a GPU.**

12. **PostgreSQL must remain optional.**

13. **Redis or another external cache must remain optional.**

14. **Authentication providers must remain replaceable and must not be hardcoded into the core architecture.**

15. **SDKs must provide integration interfaces and must not become alternative implementations of the processing engine.**

16. **No new infrastructure dependency should be introduced without a documented technical reason.**

17. **Performance claims must be supported by benchmarks.**

18. **The system must support continuous operation and controlled shutdown.**

19. **AI providers must be isolated behind an abstraction so that changing the AI provider does not require redesigning the event engine.**

20. **The system must prioritize local processing when practical.**

21. **The architecture must remain small and understandable unless additional complexity is justified by a real requirement.**

22. **The public developer API must remain simpler than the internal implementation.**

23. **No feature may silently introduce a mandatory cloud dependency.**

24. **No public API behavior may be invented or changed without updating the relevant documentation and tests.**

25. **Deltu must remain primarily a continuous event/state processing engine, not a chatbot or general-purpose AI agent.**

---

# Architectural Decision Principles

When deciding whether to add a technology or dependency, ask:

1. Is it required by the core product?
2. Can the Rust engine solve the problem without it?
3. Can it be an optional adapter instead?
4. Does it increase CPU/RAM/network requirements?
5. Does it make self-hosting harder?
6. Does it create a mandatory cloud dependency?
7. Does it make installation more complicated?
8. Does it improve the developer experience enough to justify the complexity?
9. Can users choose their own alternative?
10. Can the same functionality remain available through the SDK/API?

If a dependency is not necessary, **do not make it part of the core.**

---

# Final Architecture Statement

Deltu is a **small, open-source, self-hosted event and state processing engine**.

Its core is a Rust runtime.

Developers keep their existing applications, frameworks, databases, authentication systems, and infrastructure.

They connect to Deltu through:

```text
SDK
API
MQTT
Adapters
```

The core installation should be capable of:

```text
Input
  ↓
Event
  ↓
Process
  ↓
State
  ↓
Rule
  ↓
AI when needed
  ↓
Action
```

without requiring:

```text
Clerk
PostgreSQL
Redis
Next.js
Kubernetes
Cloud AI
GPU
```

Those technologies may be supported as **optional integrations**, but they are not Deltu itself.

> **Deltu should fit into other people's systems, not force other people's systems to fit into Deltu.**
