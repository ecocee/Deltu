# Deltu Use Cases (Audit 2026-09-26)

Status: **Supported** (verified with the current implementation),
**Supported with limits** (works; documented constraint), **Future**
(planned, not built). Nothing here is claimed without evidence.

The audit verified each use case against the engine's actual behavior —
either through the audit's own live tests (HTTP e2e, MQTT e2e with a real
broker, restart/restore, sustained load) or the 170-test suite.

## 1. Sensor / IoT telemetry — **Supported**

Device streams over MQTT or HTTP; dedupe, window, threshold, alert.

- Verified: real-broker e2e — `sensors/esp32-1/door.state` → rule →
  structured log action; numeric telemetry accumulates into windows and
  exposes mean/min/max/last to rules; reconnect backoff survives broker
  outages; `examples/demo.sh` reproduces the chain in one command.
- Components exercised: MQTT adapter, filter/dedup/aggregate, state,
  rules, log action, status counters.

## 2. Machine / device state tracking — **Supported**

Track current state (door open/closed, machine running/idle) and react
to transitions.

- Verified: state store holds `(source, kind)` → current value with
  `previous_value`; rules read `Field::State` / `Field::EventValue`;
  change detection suppresses noise below `min_delta`.
- Limits: "went offline" (absence) detection needs state-trigger rules
  evaluated periodically — state triggers fire on inputs only today
  (spec 05); a scheduler for periodic evaluation is Future.

## 3. Application & API event streams — **Supported**

Backend services POST JSON batches over HTTP (or use the SDKs) and
reduce noise before anything downstream runs.

- Verified: 2000-event burst at ~118k ev/s on the release build with
  queue drained to zero; per-item accept array; typed 400/413/429/503
  errors; Python/TypeScript SDKs tested live against a real engine.

## 4. Server / infrastructure metrics — **Supported with limits**

Numeric metrics (CPU, latency, queue depth) → windows → threshold or
change rules.

- Verified: aggregation of numeric streams with window stats and
  change-detection deadband.
- Limits: no pull/scrape input (e.g. Prometheus endpoint) — sources
  must push. A scraper adapter is Future.

## 5. Log / business-event filtering — **Supported with limits**

High-volume event logs filtered by kind and deduplicated by id before
rules or storage.

- Verified: kind-family filter + bounded dedup (10k id cache default)
  with observable counters.
- Limits: no built-in text search/parsing of free-form log lines —
  senders must emit structured JSON (text/structured payloads).

## 6. Anomaly / change detection — **Supported with limits**

Detect meaningful movement in a noisy signal.

- Verified: `min_delta` deadband suppresses sub-threshold wiggle and
  counts suppressions; window stats (min/max/mean/last) reach rules.
- Limits: thresholds are static (no learned baselines); ML-based
  anomaly scoring arrives with the Future AI runtime features.

## 7. Local/edge processing — **Supported**

Single ~5 MB binary, ~10 MB RSS under sustained 47k ev/s load, no
database, no cloud, offline-capable (MQTT+HTTP are local-network
protocols; snapshots are local files).

- Verified: sustained-load measurement on Apple Silicon release build;
  restart→restore e2e; Docker image (distroless) built in CI; ARM64
  release artifacts defined in the workflow (Linux ARM64 via `cross`).
- Limits: Raspberry Pi (ARM64 Linux) is a build-target claim, not yet a
  physically-verified device test.

## 8. AI-assisted decisions when deterministic rules are not enough — **Supported (boundary), runtimes Future**

The principle "process data first; use AI only when necessary" is
enforced structurally: rules run first; AI actions are reachable only
through the dispatcher's policy gate; usage is metered.

- Verified: scripted provider + policy tests; usage counters in
  `/v1/status`; default build contains zero AI code.
- Not yet: real ONNX/llama.cpp/cloud runtimes (Future, feature-gated).

## 9. Automated actions (webhooks) — **Future**

The success-criteria chain ends in "Webhook", but the only output
action today is the structured log. Until the webhook action ships,
integrate by pointing consumers at the engine's API or by tailing
structured logs. This is the highest-priority functional gap (see
KNOWN-ISSUES.md).

## Explicitly not supported today

- Authentication/authorization on the engine API (front with a proxy).
- Multi-node clustering, Kafka/Redis/cloud services (by design; add
  adapters when a requirement exists).
- Kubernetes/Helm (by design — infra on demand).
- Windows binaries.
- Dashboards/UI.
