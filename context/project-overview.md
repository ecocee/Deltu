# Deltu

## Overview

Deltu is an open-source, self-hosted, SDK-first continuous data processing engine that receives continuous data from applications, sensors, devices, APIs, logs, and other systems, processes that data locally, converts raw streams into meaningful events and current state, evaluates deterministic rules, and triggers actions. When a problem requires intelligence beyond deterministic processing, Deltu can optionally use local ML, local GenAI, or cloud AI. The core engine is written in Rust and is designed to run as a lightweight standalone process on Linux, x86_64, ARM64, edge devices, servers, or Docker. Developers do not need to replace their existing application stack; they connect their applications to Deltu through HTTP, MQTT, Python SDK, TypeScript SDK, or optional adapters.

## Goals

1. Build a lightweight Rust engine capable of continuously processing incoming data without requiring PostgreSQL, Redis, Clerk, Next.js, Kubernetes, or cloud infrastructure.

2. Convert high-volume raw data into useful events, state changes, patterns, and actions instead of processing every raw input unnecessarily.

3. Provide deterministic processing capabilities including filtering, deduplication, aggregation, change detection, state management, and rule evaluation.

4. Make AI optional so that normal processing can happen without an AI model and AI is invoked only when it provides meaningful additional value.

5. Support local AI/ML execution through technologies such as ONNX Runtime and llama.cpp/GGUF when local inference is appropriate.

6. Provide simple Python and TypeScript SDKs so existing applications can integrate Deltu without adopting Deltu's internal technology stack.

7. Provide a language-independent HTTP API so applications written in other languages can integrate with Deltu.

8. Support continuous data sources such as HTTP and MQTT in the initial version, with additional protocols available through future adapters.

9. Support self-hosted deployment as a single native Rust binary with Docker as an optional deployment method.

10. Keep external infrastructure optional so users can add PostgreSQL, Redis, cloud AI, dashboards, or other services only when their deployment actually requires them.

11. Measure resource usage and processing performance using events per second, latency, CPU, memory, queue depth, errors, and AI usage rather than relying on unsupported performance claims.

12. Release the project as open source with clear documentation, examples, tests, and a structure that allows developers to use, modify, and extend it for their own projects.

## Core User Flow

### Developer Integration Flow

1. A developer installs Deltu on their own machine, server, edge device, or Docker environment.

2. The developer starts the Deltu runtime.

3. The developer connects an existing application or data source using the HTTP API, MQTT, Python SDK, TypeScript SDK, or an available adapter.

4. The external system sends continuous data into Deltu.

5. Deltu validates and normalizes the incoming data into its internal event model.

6. The event enters the processing pipeline.

7. Deltu filters unnecessary data and removes duplicate or irrelevant events where configured.

8. Deltu aggregates continuous data when aggregation is required.

9. Deltu detects meaningful changes and converts raw activity into useful events.

10. Deltu updates its current runtime state.

11. The rule engine evaluates the updated state and meaningful events against configured deterministic rules.

12. If a rule can resolve the situation, Deltu performs the configured action without using AI.

13. If additional intelligence is required, Deltu sends the processed information to the configured AI/ML provider.

14. AI/ML returns a structured result that can be used by the processing system.

15. Deltu executes the configured action, such as a webhook, HTTP request, MQTT message, log event, or another adapter.

16. Deltu records relevant metrics such as processing latency, throughput, errors, resource usage, and AI usage.

17. The connected application receives the resulting event or action and continues its own workflow.

### Example Edge Flow

```text
ESP32 Sensor
     ↓
MQTT
     ↓
Deltu
     ↓
Validation
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
Webhook / MQTT Action
```

### Example AI Flow

```text
Continuous Data
     ↓
Deltu Processing
     ↓
Meaningful Event
     ↓
State / Rule
     ↓
AI Required
     ↓
Local AI or Cloud AI
     ↓
Structured Result
     ↓
Action
```

## Features

### Continuous Event Processing

* Receive continuous data through HTTP and MQTT.
* Convert external data into a common internal event model.
* Validate incoming events before processing.
* Normalize different input formats.
* Process events continuously without requiring a database.
* Support controlled buffering and backpressure.

### Data Reduction

* Filter irrelevant events.
* Deduplicate repeated events.
* Aggregate continuous measurements.
* Detect meaningful changes.
* Reduce unnecessary downstream processing.
* Prevent unnecessary AI calls.

### State Engine

* Maintain current runtime state in memory.
* Track state changes and transitions.
* Maintain counters and processing windows.
* Provide state information to rules and downstream actions.
* Keep runtime state separate from historical persistence.

### Rule Engine

* Define deterministic conditions.
* Evaluate rules against events and state.
* Trigger actions when conditions are satisfied.
* Handle conditions without requiring AI.
* Keep rule evaluation predictable and testable.

### Action Engine

* Trigger HTTP requests.
* Send webhooks.
* Publish MQTT messages.
* Produce structured logs.
* Support future action adapters.
* Isolate action failures from the core runtime.

### AI and ML

* Provide an optional AI provider interface.
* Support local ML through ONNX-compatible runtimes.
* Support local GenAI through llama.cpp/GGUF-compatible runtimes.
* Support optional cloud AI providers.
* Invoke AI only after normal processing determines it is useful.
* Track AI calls, latency, and token usage where applicable.
* Allow AI providers to be changed without redesigning the core engine.

### SDKs

* Python SDK for Python applications.
* TypeScript/JavaScript SDK for Node.js and web applications.
* Simple event submission APIs.
* Simple state and result access.
* Consistent error handling.
* SDK examples for common integrations.
* SDKs communicate with Deltu rather than duplicating the core engine.

### HTTP API

* Event ingestion API.
* Health endpoint.
* Runtime status endpoint.
* State access where exposed by the public API.
* Action and rule interfaces as they become part of the supported API.
* Language-independent integration.

### MQTT Integration

* Receive sensor/device events.
* Publish configured actions.
* Support continuous device data.
* Handle connection failures and reconnection.
* Convert MQTT messages into the Deltu event model.

### Local and Edge Deployment

* Native Linux binary.
* ARM64 support.
* x86_64 support.
* Raspberry Pi deployment.
* Docker deployment.
* Offline-capable processing where the configured features permit it.
* Local AI support without mandatory cloud connectivity.

### Observability

* Structured logging.
* Event processing metrics.
* Processing latency measurements.
* Throughput measurements.
* Queue and backpressure metrics.
* Error metrics.
* CPU and memory measurements where available.
* AI call and token metrics.

### Optional Storage

* In-memory runtime state by default.
* Optional local persistence.
* Optional PostgreSQL adapter for historical data.
* Optional external cache adapter when distributed deployments require it.
* Local filesystem support for configuration, models, logs, exports, and artifacts.

### Optional Interfaces

* Optional web dashboard.
* Optional MCP adapter.
* Optional external database integrations.
* Optional external cache integrations.
* Optional cloud AI integrations.
* Optional future protocol adapters.

## Scope

### In Scope

* Rust-based Deltu core engine.
* Continuous event processing.
* Common event model.
* Input validation and normalization.
* HTTP event ingestion.
* MQTT integration.
* Buffering and controlled queues.
* Filtering.
* Deduplication.
* Aggregation.
* Change detection.
* Runtime state management.
* Deterministic rule evaluation.
* Webhook actions.
* HTTP actions.
* MQTT actions.
* Structured logging.
* Runtime metrics.
* Error handling.
* Backpressure.
* Graceful shutdown.
* Python SDK.
* TypeScript/JavaScript SDK.
* Public HTTP API.
* Local ML provider architecture.
* Optional ONNX Runtime integration.
* Optional local GenAI provider architecture.
* Optional llama.cpp/GGUF integration.
* AI provider abstraction.
* AI usage and token tracking.
* Linux x86_64 support.
* Linux ARM64 support.
* Raspberry Pi deployment.
* Optional Docker deployment.
* Example applications.
* Integration tests.
* End-to-end tests.
* Performance benchmarks.
* Developer documentation.
* Self-hosted installation documentation.
* Open-source repository structure.
* Optional PostgreSQL adapter.
* Optional external cache adapter.
* Optional dashboard architecture.
* Optional MCP adapter architecture.

### Out of Scope

* Mandatory user authentication platform.
* Mandatory Clerk integration.
* Mandatory PostgreSQL.
* Mandatory Redis.
* Mandatory Next.js application.
* Mandatory NestJS backend.
* Mandatory Kubernetes deployment.
* Mandatory cloud infrastructure.
* Mandatory cloud AI.
* Mandatory GPU infrastructure.
* Building a proprietary cloud SaaS platform as the core product.
* Requiring users to replace their existing application framework.
* Reimplementing the user's application authentication, billing, or organization management.
* Making Deltu a general-purpose chatbot.
* Making Deltu a general-purpose autonomous AI agent by default.
* Sending every incoming event to an LLM.
* Storing every incoming event permanently by default.
* Building a large distributed architecture before actual scale requirements exist.
* Building an enterprise dashboard before the core engine is functional.
* Adding Kafka or similar streaming infrastructure without a concrete requirement.
* Adding a vector database without a concrete feature that requires it.
* Making any particular AI provider mandatory.
* Making any particular database or cache provider mandatory.

## Success Criteria

1. Deltu can start as a standalone Rust process without PostgreSQL, Redis, Clerk, Next.js, or cloud infrastructure.

2. A developer can send an HTTP event to Deltu and receive a valid processing result.

3. A developer can connect an MQTT data source and continuously send events into Deltu.

4. Incoming events are validated and converted into the common internal event model.

5. The processing pipeline can filter, deduplicate, aggregate, and detect meaningful changes.

6. The state engine can maintain current runtime state without requiring a database.

7. The rule engine can evaluate deterministic conditions and trigger an action.

8. A failed external action does not terminate the entire Deltu runtime.

9. The system handles continuous processing without unbounded memory or queue growth under its defined operating limits.

10. A Python application can connect to Deltu using the Python SDK.

11. A TypeScript/JavaScript application can connect to Deltu using the TypeScript SDK.

12. An application written in another language can integrate with Deltu through the HTTP API.

13. AI is not required for the core event-processing pipeline to function.

14. When AI is configured, the system can invoke it only after the normal processing pipeline determines that AI is required.

15. The AI provider can be replaced without redesigning the event, state, rule, and processing engines.

16. Deltu can run on both x86_64 and ARM64 Linux environments.

17. The core engine can run on a Raspberry Pi within the defined resource and performance targets.

18. Core functionality is covered by appropriate unit and integration tests.

19. Performance-sensitive components have benchmark results for throughput, latency, CPU, and memory usage.

20. The repository contains working examples showing how an external application connects to Deltu.

21. The installation and self-hosting process is documented and reproducible.

22. The core architecture remains independent of specific frontend, authentication, database, cache, and cloud vendors.

23. Optional PostgreSQL, cache, AI, dashboard, and MCP integrations can be added without making them mandatory dependencies of the core engine.

24. The implementation, architecture documentation, workflow rules, and progress tracker remain synchronized.

25. The first complete MVP successfully demonstrates:

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

26. The final MVP demonstrates the central Deltu principle:

> **Process data first. Use AI only when necessary.**
