# Spec 11 — Local AI (Optional Layer)

Status: DRAFT (pre-drafted on request; finalize at unit start) · Depends on: Units 03–10 (complete deterministic core) · Optional: engine runs fully without this unit

## Goal

Add the AI provider abstraction and the first local providers (research/ai.md),
honoring decision 003: AI is never on the mandatory path, is invoked only
after deterministic processing, and receives meaningful compressed
information — never raw event streams (invariants 5, 6, 19, 23).

## Design

### Module

`core/ai/` maps to `src/ai/`:

```text
src/ai/
├── mod.rs        # AiProvider trait, AiManager, invocation policy, re-exports
├── policy.rs     # when AI may be called (explicit rule-produced requests only)
├── onnx.rs       # OnnxProvider (feature "ai-onnx")
└── llamacpp.rs   # LlamaCppProvider (feature "ai-llamacpp")
```

### Abstraction

```rust
pub trait AiProvider: Send {
    fn infer(&mut self, request: AiRequest) -> Result<AiResult, AiError>;
    fn name(&self) -> &str;
    // AiRequest: structured features + prompt (compressed, rule-selected)
    // AiResult: structured output + usage (tokens/latency/model)
}
```

* **Invocation policy (the unit's core invariant):** only rule matches whose
  action is `ai` reach a provider — the pipeline (03) → rules (05) chain
  fully determines when AI runs. No adapter, timer, or default path may call
  AI. The policy is enforced by construction: the AI manager is reachable
  *only* from the action dispatcher's `ai` action type.
* **Usage tracking (mandatory features of the boundary):** calls,
  calls-avoided (rule matches that skipped AI because a deterministic action
  resolved), latency, tokens, model name, local-vs-cloud flag — wired into
  the Unit 10 registry.
* **Feature-gated builds:** `ai-onnx` (via `ort`, ONNX Runtime ~1.28 line)
  and `ai-llamacpp` (via `llama-cpp-2` OR llama.cpp local-server HTTP —
  chosen at implementation after measuring cross-compile cost on ARM64 per
  research/edge.md). Default build compiles **neither**; `cargo build`
  remains dependency-light (decision 003; dependency rule).
* Failure isolation: AI errors are `ActionOutcome`-style results, counted,
  never crash the runtime; timeouts mandatory.
* No cloud provider in this unit (12 covers it); nothing here requires
  network access — local-first (invariant 20).

## Implementation

1. Trait + policy + manager + usage metrics wiring; tests with a scripted
   in-memory provider (no model needed for CI).
2. `OnnxProvider` behind `ai-onnx` with a tiny bundled classification model
   in CI only; `LlamaCppProvider` behind `ai-llamacpp` against a local
   server or bindings per implementation findings.
3. `ai` action type end-to-end: rule → dispatcher → provider → structured
   result into the pipeline (documented feedback shape).
4. Benchmark: inference latency excluded (model-dependent); measured are
   request-building + usage accounting overhead.
5. Scope guard: no model downloads at build time, no GPU requirement
   (invariant 11), no cloud calls, no AI in default features.

## Dependencies

Feature-gated only: `ort` (ai-onnx), `llama-cpp-2` (ai-llamacpp) — each
justified against the dependency rule and the edge research at
implementation time. Core dependencies unchanged.

## Verify When Done

* [ ] Default build: no AI crates compiled, all prior tests green.
* [ ] Feature builds compile on x86_64 and (at least plan) ARM64.
* [ ] Policy tests prove AI cannot be invoked outside rule-produced requests.
* [ ] Usage counters verified; tracker updated (results + notes).
