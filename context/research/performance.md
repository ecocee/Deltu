# Topic — Performance

## Question

How should Deltu measure throughput, latency, and resource usage so that
performance claims are backed by data from the first functional units onward?

## Findings

* **Metrics that matter** for a continuous engine: events per second,
  average/P95/P99 latency, CPU %, RSS memory, queue depth, dropped events,
  failed actions, startup time, sustained throughput over time. AI metrics
  (calls, tokens, calls avoided) join this list when AI is introduced.
* **Latency percentiles**: averages hide tail behavior. P95/P99 are the
  numbers that reveal overload and blocking. Measure per stage where
  practical (validation, pipeline, state, rules, action dispatch), not
  only end-to-end.
* **Benchmark methodology**: benchmarks must run on a stable, otherwise-idle
  machine, with warm-up excluded, and report distributions rather than
  single runs. For Rust, `criterion` is the de-facto standard harness
  (statistical comparison between runs); it belongs in `benchmarks/` once
  processing stages exist — not in Unit 01.
* **Queue depth and backpressure signals**: sustained queue growth means
  throughput < input rate; that condition must be visible (metric + log),
  not silently absorbed by an unbounded queue.
* **Allocation awareness**: allocation churn is measurable with heap
  profilers (e.g., `dhat` for targeted measurement); regular profiling of
  hot stages is worthwhile only once benchmarks exist.
* **No unsupported claims**: numbers are produced by the benchmark harness
  on named hardware; "fast/lightweight" without a benchmark is prohibited
  by code standards.

## Sources

* criterion crate — https://docs.rs/criterion and https://github.com/bheisler/criterion.rs
* dhat-rs — https://docs.rs/dhat
* Gil Tene, "How NOT to Measure Latency" (latency measurement pitfalls)
* Code standards — performance standards (`context/code-standards.md`)

## Impact on Deltu

* The metrics unit (Unit 10) defines the metrics interface; until then,
  units record what is measurable cheaply (tests, CI timing) without
  pretending to be benchmarks.
* Benchmarks are introduced when processing stages exist (Unit 03 onward),
  in `benchmarks/`, using `criterion`; results must state hardware and
  conditions.
* Every performance statement in documentation traces to a benchmark
  result or is removed.

## Decision

Deltu adopts criterion-based benchmarks for processing stages as soon as
they exist, tracks throughput/latency/CPU/memory/queue-depth metrics
end-to-end, and forbids performance claims that are not backed by recorded
benchmark results.
