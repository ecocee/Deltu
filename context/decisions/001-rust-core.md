# 001 — Rust Core Engine, Single Self-Hosted Binary

Status: Accepted · Date: 2026-09-25

## Decision

The Deltu engine is written in Rust and distributed as a single native
binary that runs on Linux x86_64 and ARM64 (including Raspberry Pi and
Docker) without external infrastructure.

## Reason

Deltu is a continuous data processing engine where low resource usage,
predictable latency, and standalone operation are product requirements, not
optimizations. Rust provides memory safety without a garbage collector, a
small deployment footprint (a single static-ish binary), and first-class
cross-compilation to ARM64. Self-hosting must remain trivial: copy a binary,
run it.

## Alternatives Considered

* **Node.js/TypeScript engine** — familiar ecosystem, but higher memory and
  CPU baseline, and a runtime dependency on Node for "self-hosted" users.
* **Python engine** — fast to prototype, but throughput, latency
  predictability, and distribution (a single self-contained binary) are
  significantly worse; Python is retained for AI experimentation and SDK
  work only.
* **Go engine** — viable single-binary story, but Rust's stronger type
  system and control over allocations fit the bounded-memory processing
  requirements better.

## Consequences

* Development moves in small verified Rust units per the build plan.
* Pure-Rust dependencies are preferred to keep ARM64 cross compilation
  simple (see `research/edge.md`).
* SDKs (Python, TypeScript) and the dashboard integrate via the public API;
  they never reimplement engine behavior.
