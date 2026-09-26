![Deltu](assets/banner.png)

A lightweight, local-first event-processing engine in Rust that turns continuous data into **meaningful events, state, rules, and actions**.

```text
Install ──> deltu demo ──> deltu init ──> deltu run ──> Send Events ──> Trigger Actions
```

**No cloud required. No database required. No AI required. One small binary.**

---

## ⚡ 5-Minute Quickstart

### 🤖 AI Agent Installation

Want an AI coding agent to install DELTU for you? Paste this prompt into Codex, Claude Code, Cursor, or your favorite agent:

```text
Please install the DELTU event-processing engine in my environment.

DELTU is a lightweight, local-first event processor.
Repository: https://github.com/ecocee/Deltu

1. Inspect my OS and CPU architecture.
2. The official installation script is at: https://raw.githubusercontent.com/ecocee/Deltu/main/scripts/install.sh
3. Run the installer safely: `curl -fsSL https://raw.githubusercontent.com/ecocee/Deltu/main/scripts/install.sh | sh`
4. The installer places the binary at `~/.deltu/bin/deltu`. Ensure this path is added to my shell's PATH, or use the absolute path for the next steps.
5. Verify the installation by running `deltu version` (or `~/.deltu/bin/deltu version`).
6. Run `deltu doctor` to ensure the system is ready.
7. Run `deltu demo` to verify the engine works.
8. If the installation fails due to an unsupported architecture, try building from source by cloning the repository and running `cargo build --release`.
9. Report back the final status, the version installed, and output of the demo. Do not invent release URLs or binaries.
```

### 1. Manual Install
```bash
curl -fsSL https://raw.githubusercontent.com/ecocee/Deltu/main/scripts/install.sh | sh
```
*To uninstall at any time: `curl -fsSL https://raw.githubusercontent.com/ecocee/Deltu/main/scripts/uninstall.sh | sh`*

### 2. See DELTU in action
```bash
deltu demo
```

### 3. Create your configuration
```bash
deltu init
```
Generates a `deltu.yaml` with simple human-readable rules:
```yaml
http:
  bind: 127.0.0.1:8080

rules:
  - name: high-temperature
    when: temperature > 80
    within: 60s
    action: log-alert

  - name: api-error-storm
    when: count(http.error) > 10
    within: 30s
    action: log-alert

actions:
  - id: log-alert
    kind:
      type: log
      level: warn
```

### 4. Validate & Start
```bash
deltu check --config deltu.yaml
deltu run --config deltu.yaml
```

### 5. Send an event
```bash
curl -X POST http://127.0.0.1:8080/v1/events \
  -H 'content-type: application/json' \
  -d '{
    "events": [{
      "id": "e1",
      "source": "sensor-1",
      "kind": "temperature",
      "timestamp": 1700000000000,
      "payload": {
        "type": "numeric",
        "value": 86
      }
    }]
  }'
```

---

## The problem

Applications continuously produce data.

Sensors produce readings.
APIs produce events.
Servers produce metrics.
Devices produce state changes.
Business systems produce transactions.

Most applications then end up implementing the same plumbing:

```text
filter
deduplicate
aggregate
detect changes
maintain state
evaluate rules
trigger actions
```

DELTU provides that runtime as one small, self-hostable engine.

### The principle

> **Process data first. Use AI only when necessary.**

If deterministic code can understand the event, let deterministic code handle it.

AI can be connected when a decision actually requires it.

---

## What DELTU does

```mermaid
flowchart LR
    A[Continuous Data] --> B[DELTU]

    B --> C[Filter]
    C --> D[Deduplicate]
    D --> E[Aggregate]
    E --> F[Change Detection]
    F --> G[State]
    G --> H[Rules]
    H --> I[Actions]

    H -. optional .-> J[AI]
    J -.-> I
```

The core engine does not require AI.

It does not require Kubernetes.

It does not require Kafka.

It does not require Redis.

It does not require a cloud service.

It is designed to be useful by itself.

---

## What can you build?

DELTU is not limited to IoT.

```mermaid
flowchart TD
    D[DELTU]

    D --> S[Sensors & Devices]
    D --> A[Applications & APIs]
    D --> O[Servers & Infrastructure]
    D --> B[Business Processes]

    S --> S1[Threshold alerts]
    A --> A1[Event processing]
    O --> O1[Infrastructure monitoring]
    B --> B1[State transitions]
```

Examples:

* Sensor and telemetry alerts
* Application event processing
* API event reduction
* Infrastructure monitoring
* Device and job state tracking
* Edge and offline-first processing
* Webhook automation
* Event-driven business workflows
* Stateful rules and notifications

---

## Verified numbers

| Metric               |                     Value |
| -------------------- | ------------------------: |
| Ingest throughput    |             ≈77k events/s |
| Latency p95 / p99    |               1 ms / 1 ms |
| Release binary       |                    5.2 MB |
| Cold start → healthy |                   ≈0.65 s |
| Rust tests           |                       166 |
| SDK tests            | 12 Python + 12 TypeScript |
| AI required          |                        No |
| Cloud required       |                        No |

Benchmarks are environment-dependent; see the benchmark documentation for methodology.

---

## Quick start

### 1. Install

**Release binary**

```bash
# Download from GitHub Releases
deltu run
```

**From source**

```bash
cargo run --release -- run
```

**Docker**

```bash
docker build -t deltu:local .

docker run -d \
  -p 8080:8080 \
  deltu:local run
```

### 2. Check the engine

```bash
curl http://127.0.0.1:8080/health
```

### 3. Send an event

```bash
curl -X POST http://127.0.0.1:8080/v1/events \
  -H 'content-type: application/json' \
  -d '{
    "events": [{
      "id": "e1",
      "source": "sensor-1",
      "kind": "door.state",
      "timestamp": 1700000000000,
      "payload": {
        "type": "text",
        "value": "open"
      }
    }]
  }'
```

### 4. Inspect the engine

```bash
curl http://127.0.0.1:8080/v1/status | python3 -m json.tool
```

---

## Inputs

* HTTP
* MQTT
* Python SDK
* TypeScript SDK

## Processing

* Filtering
* Deduplication
* Aggregation
* Change detection
* Stateful processing
* Rules
* Suppression windows

## Actions

* Log
* Webhook
* Optional AI provider

---

## Architecture

```mermaid
flowchart LR
    subgraph INPUT["Input Boundary"]
        HTTP["HTTP"]
        MQTT["MQTT"]
        SDK["SDKs"]
    end

    INPUT --> Q[("Bounded Queue")]

    Q --> W["Worker"]

    W --> P["Processing Pipeline"]

    P --> F["Filter"]
    F --> D["Dedup"]
    D --> A["Aggregate"]
    A --> C["Change Detection"]

    C --> S[("State")]

    S --> R["Rules"]

    R --> AC["Actions"]

    AC --> L["Log"]
    AC --> WH["Webhook"]
    AC -. optional .-> AI["AI"]

    W -.-> M["Metrics"]
    S -.-> M
    R -.-> M
    AC -.-> M
```

The input boundary validates and enqueues events.

The core worker processes them synchronously.

The bounded queue provides the backpressure boundary.

Slow network actions are isolated from the processing pipeline.

---

## Why DELTU?

### Bounded by construction

Queues, caches, windows, and state stores have explicit limits and eviction policies.

### Deterministic core

The processing core is synchronous and clock-free. Wall time enters only at the runtime boundary.

### Failures are observable

A failed webhook or full queue does not silently disappear.

Failures are counted and exposed through the status API.

### AI is optional

The default engine does not require an AI provider.

AI can be introduced only where it provides value.

### One binary

A small Rust runtime that can run on a server, edge machine, or Raspberry Pi.

---

## SDKs

### Python

```python
from deltu import DeltuClient, Event, text
import time

with DeltuClient("http://127.0.0.1:8080") as client:
    client.send_event(
        Event(
            id="e1",
            source="sensor-1",
            kind="door.state",
            timestamp=int(time.time() * 1000),
            payload=text("open"),
        )
    )
```

### TypeScript

```typescript
import { DeltuClient, Event, text } from "@deltu/client";

const client = new DeltuClient("http://127.0.0.1:8080");

await client.sendEvent(
  new Event({
    id: "e1",
    source: "sensor-1",
    kind: "door.state",
    timestamp: Date.now(),
    payload: text("open"),
  })
);
```

---

## HTTP API

| Route             | Purpose                     |
| ----------------- | --------------------------- |
| `POST /v1/events` | Ingest events               |
| `GET /health`     | Health check                |
| `GET /v1/status`  | Runtime status and counters |

Example error:

```json
{
  "success": false,
  "error": {
    "code": "invalid_event",
    "message": "..."
  }
}
```

---

## Configuration

Deltu is configured through `engine.yaml`.

```bash
deltu check --config engine.yaml
```

Validate first.

Then deploy:

```bash
deltu run --config engine.yaml
```

For background operation:

```bash
deltu up --config engine.yaml
```

Inspect:

```bash
deltu status
deltu logs
```

Stop:

```bash
deltu down
```

Full deployment guide: [`docs/deploy.md`](docs/deploy.md)

---

## Development

```bash
cargo test --lib
cargo clippy --all-targets
cargo bench
cargo run --release -- run
```

CI runs formatting, clippy, tests, release builds, and Docker smoke tests.

Release builds publish Linux x86_64, Linux ARM64, and macOS ARM64 artifacts.

---

## Open source

Deltu is licensed under the **Apache License 2.0**.

* [`LICENSE`](LICENSE)
* [`NOTICE`](NOTICE)
* [`CONTRIBUTING.md`](CONTRIBUTING.md)
* [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md)
* [`SECURITY.md`](SECURITY.md)
* [`TRADEMARKS.md`](TRADEMARKS.md)

The DELTU name and logo are trademarks of ECOCEE; the software itself is Apache-2.0 licensed.

---

## Build with DELTU

Have an interesting use case?

Build something with Deltu and share it.

**Applications · Edge systems · Automation · Infrastructure · Devices · Developer tools**

Open an issue, start a discussion, or submit a pull request.

**DELTU**

> **Make Data Behave.**
