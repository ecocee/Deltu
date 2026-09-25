# Topic — AI

## Question

How should Deltu support AI/ML — local ML, local GenAI, and cloud AI — as a
strictly optional layer that never becomes a dependency of the deterministic
core?

## Findings

* **Provider abstraction**: the engine must depend on an interface, not on a
  specific runtime or vendor. The interface receives already-processed,
  meaningful information (not raw event streams) and returns a structured
  result. Swapping providers must not require redesigning the event, state,
  rule, or processing engines (invariant 19).
* **Local ML — ONNX Runtime**: ONNX is an open model format with
  cross-runtime support. The established Rust binding is the `ort` crate
  (current 2.0.0-rc line wrapping ONNX Runtime ~1.28). It supports CPU
  execution without a GPU, which matters for edge targets. The rc-status
  versioning is a known risk to reassess when the AI unit lands.
* **Local GenAI — llama.cpp / GGUF**: GGUF is llama.cpp's single-file model
  format; llama.cpp runs quantized LLMs efficiently on CPU. Rust bindings
  exist (`llama-cpp-2`, actively tracking upstream), but llama.cpp moves
  fast and bindings are inherently coupled to that churn. An alternative
  is running llama.cpp as a separate local server process and speaking to it
  over HTTP, which isolates the Rust runtime from the native dependency —
  at the cost of a second process.
* **Cloud AI**: any provider can sit behind the same abstraction. No cloud
  provider may become a mandatory dependency (invariant 23).
* **Invocation discipline**: incoming events must not automatically be sent
  to AI (invariant 5). The deterministic pipeline (filter → aggregate →
  change detect → state → rules) runs first; AI is invoked only for the
  residual cases that rules/statistics cannot resolve.
* **Usage tracking**: AI calls, calls avoided, latency, tokens, model
  identity, and local-vs-cloud execution must be tracked — both for cost
  control and to verify the "AI only when necessary" principle is real.

## Sources

* ONNX Runtime documentation — https://onnxruntime.ai/
* ort crate — https://docs.rs/ort and https://ort.pyke.io/
* llama.cpp — https://github.com/ggml-org/llama.cpp
* llama-cpp-2 crate — https://docs.rs/llama-cpp-2 and https://github.com/utilityai/llama-cpp-rs
* ONNX format — https://onnx.ai/

## Impact on Deltu

* AI units come late (11+) per the build order; nothing AI-related enters
  `Cargo.toml` before then.
* The AI boundary will be defined as a single provider trait fed by
  processed events with structured results; deterministic processing never
  calls it implicitly.
* Local-first preference: ONNX for tiny/classic ML, llama.cpp (bindings or
  local server) for GenAI — both CPU-capable, matching the edge model.
* Metrics must include AI calls, tokens, latency, and calls avoided from
  the day AI is introduced.

## Decision

AI is optional, isolated behind one provider abstraction, invoked only after
deterministic processing, and tracked with usage metrics. Local runtimes
(ONNX via `ort`; llama.cpp via bindings or local server) are preferred over
cloud providers; no provider is mandatory.
