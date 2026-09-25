# Topic — MQTT

## Question

How should Deltu receive and publish MQTT data so sensor and IoT streams
integrate reliably, survive broker disconnections, and respect backpressure?

## Findings

* **Protocol**: MQTT is a lightweight pub/sub protocol over TCP. Clients
  publish and subscribe on hierarchical topics. QoS 0 (at most once), QoS 1
  (at least once), and QoS 2 (exactly once) trade overhead against delivery
  guarantees; QoS 1 is the usual default for telemetry that must not be lost.
* **Retained messages**: the broker stores the last retained message per
  topic and delivers it to new subscribers — useful for "current value"
  bootstrapping, and a source of replayed events that validation and
  deduplication must tolerate.
* **Reconnection**: connections drop; clients must reconnect with backoff
  and re-establish subscriptions automatically. Sessions/clean-session
  settings determine whether the broker queues messages during downtime.
* **Backpressure**: an MQTT client must not buffer unboundedly when the
  engine is slow. Bounded in-flight limits and explicit drop/stop policies
  keep memory stable; dropped messages should be counted in metrics.
* **Payload handling**: payloads are bytes; the adapter is responsible for
  decoding, validating, and converting to the internal event model before
  the pipeline sees anything (MQTT-specific behavior stays in the adapter).
* **Rust client crates**: `rumqttc` (pure-Rust, tokio-based eventloop, MQTT
  3.1.1 and 5.0 via its `v5` module; current line 0.25.x, with maintained
  MQTT-5-oriented forks in the rumqtt family) is the established choice.
  A pure-Rust client also avoids C cross-compilation pain on ARM64.

## Sources

* MQTT Version 5.0 specification (OASIS) — https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html
* MQTT Version 3.1.1 specification (OASIS) — https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html
* rumqttc documentation — https://docs.rs/rumqttc
* rumqtt project — https://github.com/bytebeamio/rumqtt

## Impact on Deltu

* The MQTT unit (Unit 08) owns topic parsing, payload validation, conversion
  to the internal event model, reconnection policy, and QoS defaults in its
  spec; the core engine remains MQTT-free until then.
* Validation and deduplication must handle broker-replayed retained messages.
* The chosen client must support bounded buffering so a slow engine cannot
  grow memory without limit; drop counters must be exposed via metrics.

## Decision

Deltu integrates MQTT through `rumqttc` (protocol version finalized in the
Unit 08 spec — 5.0-capable client, default configured per deployment), with
QoS 1 default for ingestion, automatic reconnection with backoff, bounded
buffers, and all MQTT-specific behavior isolated in the MQTT adapter.
