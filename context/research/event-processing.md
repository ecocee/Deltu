# Topic — Event Processing

## Question

How should Deltu model, validate, and reduce continuous input data into
meaningful events without unbounded processing or memory growth?

## Findings

* **Event model**: stream-processing systems normalize all inputs into a
  common internal representation — typically identity, source, type,
  timestamp, and payload. A single internal event model is what makes
  downstream filtering, aggregation, and rule evaluation uniform.
* **Validation first**: external input is validated before entering the
  processing pipeline. Invalid input is rejected at the boundary with an
  explicit error, never silently coerced or dropped.
* **Filtering**: cheap predicate checks early in the pipeline prevent
  irrelevant data from consuming downstream resources. Order matters:
  cheapest and most selective checks first.
* **Deduplication**: bounded memory requires keyed dedup with eviction
  (e.g., TTL- or size-limited caches). Naive "remember every id" sets grow
  without bound and are unsuitable for a long-running memory-first engine.
* **Aggregation**: continuous measurements are reduced over windows —
  tumbling windows (non-overlapping) are the simplest correct starting
  point; sliding/hopping windows can come later. Windowed aggregation
  turns high-rate raw data into low-rate meaningful events.
* **Change detection**: comparing the new value against retained last-state
  (thresholds, deadbands) converts raw activity into meaningful events and
  reduces downstream work further.
* **Backpressure and bounded processing**: unbounded queues hide overload
  and grow memory; bounded queues with an explicit policy (block, drop-new,
  drop-old, or shed) keep behavior predictable. The policy must be
  observable via metrics (dropped events).
* **Event ordering**: per-source ordering is what consumers usually expect;
  global ordering across sources is neither achievable nor needed. Events
  carry timestamps, but pipeline order is arrival order within a source.

## Sources

* stream processing references (windowing, watermarking, backpressure) —
  e.g., Apache Flink concepts documentation — https://nightlies.apache.org/flink/flink-docs/
* Kleppmann, *Designing Data-Intensive Applications* (event models, ordering, backpressure)
* std::collections::HashMap / eviction considerations — https://doc.rust-lang.org/std/collections/

## Impact on Deltu

* The internal event model (Unit 02) is a single Rust struct validated at
  every adapter boundary; adapters may not create their own incompatible
  representations (invariant 9 of `architecture.md`).
* Filtering, deduplication, aggregation, and change detection (Unit 03) are
  separate pipeline stages with explicit, bounded state and documented
  eviction/TTL behavior.
* Buffering uses bounded queues with a configured overflow policy once the
  async runtime exists; until then, synchronous processing keeps behavior
  trivially bounded.

## Decision

Deltu implements its own small processing pipeline — filter → deduplicate →
aggregate → change-detect — over one validated internal event model, with
bounded state everywhere and explicit overflow policies. No external stream
framework (Kafka, Flink, etc.) is introduced without a concrete requirement.
