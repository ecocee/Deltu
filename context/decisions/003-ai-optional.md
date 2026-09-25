# 003 — AI Optional Behind a Provider Abstraction

Status: Accepted · Date: 2026-09-25

## Decision

AI is not part of the core processing path. The engine works completely
without AI. When AI is configured, it sits behind a single provider
abstraction, is invoked only after deterministic processing, and receives
processed, meaningful information — never raw event streams.

## Reason

Most conditions Deltu will face are deterministic (`temperature > 80`,
"sensor offline for 5 minutes") and are solved more reliably, cheaply, and
predictably by rules. Sending every event to a model would violate the
project's central principle — **process data first, use AI only when
necessary** — and would create cost, latency, privacy, and offline-operation
problems for edge deployments. A provider abstraction keeps any specific
runtime (ONNX, llama.cpp, cloud) replaceable without redesigning the engine.

## Alternatives Considered

* **AI inside the pipeline by default** — rejected: violates invariant 5
  (incoming data must not automatically be sent to AI) and makes the
  deterministic path depend on an optional subsystem.
* **Multiple AI-specific integrations (no abstraction)** — rejected:
  swapping providers would require engine changes, violating invariant 19.

## Consequences

* The deterministic hierarchy rules → statistics → tiny ML → local ML →
  local GenAI → cloud AI guides feature work; use the simplest sufficient
  mechanism.
* AI units are scheduled late (build order 11+); no AI crate enters
  `Cargo.toml` before then.
* Usage metrics (calls, calls avoided, tokens, latency) are required
  features of the AI boundary, not afterthoughts (see `research/ai.md`).
