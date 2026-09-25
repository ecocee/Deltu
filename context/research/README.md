# Research

This directory collects technical research that informs Deltu's architecture
and implementation decisions. Research must be captured here — not left only
in chat history or temporary agent reasoning.

Each document follows this structure:

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

## Index

| Document                                   | Topic                                     |
| ------------------------------------------ | ----------------------------------------- |
| [`rust.md`](rust.md)                       | Cargo, project structure, error handling, testing |
| [`event-processing.md`](event-processing.md) | Event models, filtering, deduplication, aggregation |
| [`http.md`](http.md)                       | HTTP server architecture and integration  |
| [`mqtt.md`](mqtt.md)                       | MQTT protocol and Rust client libraries   |
| [`state.md`](state.md)                     | In-memory runtime state design            |
| [`rules.md`](rules.md)                     | Deterministic rule evaluation             |
| [`ai.md`](ai.md)                           | Optional AI/ML providers (ONNX, llama.cpp)|
| [`edge.md`](edge.md)                       | ARM64, Raspberry Pi, cross compilation    |
| [`performance.md`](performance.md)         | Benchmark methodology and metrics         |

## Rules

- Prefer authoritative sources: official Rust/Cargo documentation, official
  crate documentation, protocol specifications, standards, reputable
  engineering references. Do not copy from random blogs.
- Research is for informing the architecture. It is not permission to add
  unnecessary technology.
- Version-sensitive facts are verified against current sources when a unit
  lands; exact dependency versions are pinned in `Cargo.lock` per unit.
