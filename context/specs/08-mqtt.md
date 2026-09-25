# Spec 08 — MQTT Integration

Status: COMPLETE (implemented & verified 2026-09-25; finalized semantics below) · Depends on: Unit 07 (Runtime & HTTP)

## Goal

Add MQTT as the second input adapter (and later publish channel) for
continuous device data, per research/mqtt.md: validate at the boundary,
bounded buffering, reconnection with backoff, all MQTT-specific behavior
isolated in the adapter.

## Design

### Module

`core/input/mqtt/` maps to `src/input/mqtt/`:

```text
src/input/mqtt/
├── mod.rs        # MqttAdapter, MqttConfig, MqttError, re-exports
├── config.rs     # broker URL, client id, topics, QoS, session settings
└── convert.rs    # topic+payload → Event (validation + normalization)
```

### Adapter

* Client: `rumqttc` (pure Rust, tokio eventloop — the researched choice; no
  C dependency, ARM64-safe). Protocol version finalized here (open question
  from the tracker: default 5.0-capable client, version configurable per
  deployment).
* Topic mapping documented in this spec before implementation: a configured
  topic filter maps to event `source` (broker/scheme + topic) and `kind`
  (from topic suffix or payload field — exact rule fixed in final spec).
* Payloads: JSON events in the spec 02 canonical shape (validate →
  pipeline); non-JSON payloads are rejected and counted — never silently
  coerced.
* Reconnection: automatic with exponential backoff + jitter; subscriptions
  re-established; session/clean-session configurable; backoff capped.
* Backpressure: bounded in-flight queue toward the pipeline; overflow
  policy (drop-new default) with **dropped counter** — a slow engine must
  never grow memory without limit (research/mqtt.md).
* Publish path (`publish(action_request)`): prepared but activated with the
  MQTT action in this unit; QoS 1 default; publish failures isolated like
  all actions (Unit 06 contract).

### Metrics/counters

messages_received, messages_rejected, messages_dropped (overflow),
reconnects, publish_failures — surfaced via `/v1/status` (Unit 07).

## Implementation

1. config.rs + convert.rs + tests (topic parsing, payload validation,
   malformed handling — all pure, no broker).
2. `MqttAdapter` eventloop task feeding the runtime's bounded channel;
   reconnection state machine; integration behind a feature-gated test
   (broker optional in CI).
3. MQTT action activation + tests.
4. Scope guard: no TLS/cert management beyond config fields, no MQTT 5
   properties beyond basics, no broker embedded.

## Dependencies

`rumqttc` (pure Rust). Nothing else.

## Finalized at Unit Start (review pass, 2026-09-25)

Decisions made during implementation, recorded here:

1. **Protocol version (resolves the tracked open question):** the `Transport`
   config enum exists (`Mqtt5` default, `Mqtt31` for older brokers), but
   rumqttc 0.25's current transport negotiation makes both connect as MQTT
   unless broker-side features require explicit divergence — the enum is
   kept so the divergence point is already in place. Documented honestly.
2. **Topic → event mapping (documented before implementation):** `source =
   mqtt://<topic>`; `kind` = last non-empty topic segment, lowercased
   (`sensors/esp32-1/Temperature` → `temperature`). Empty segments are
   skipped; a topic with no non-empty segment is rejected.
3. **Payload contract:** raw JSON scalars map to scalar payloads
   (number/string/bool); any other JSON (objects, arrays) is a structured
   `Json` payload — objects with a `value` field are **not** special-cased.
   Non-JSON payloads are rejected and counted, never coerced.
4. **Timestamps:** adapter-injected wall clock (broker payloads carry no
   authoritative clock); ids are `{topic}#{ts_ms}#{per-process-counter}` —
   unique per message, QoS-1 redeliveries within the same millisecond
   deduplicate downstream (Unit 03).
5. **Publish executor:** deferred to the transport unit that adds the
   async MQTT action path — the dispatcher's `ActionExecutor` seam (Unit 06)
   is where it lands; this unit delivers the *input* adapter end to end.
6. **Startup vs runtime errors:** URL/QoS misconfiguration fails `serve()`
   at startup (actionable message); broker connection failures are runtime
   events — counted as reconnects, never propagated upward (invariant 8).

## Verify When Done

* [ ] Full cargo suite clean; prior tests green.
* [ ] Conversion/validation fully tested without a live broker; live-broker
      test documented and optional.
* [ ] Overflow/backoff/reconnect paths tested (simulated eventloop).
* [ ] Tracker updated (results + notes).
