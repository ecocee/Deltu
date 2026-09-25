# Spec 13 — SDKs (Python, TypeScript)

Status: DRAFT (pre-drafted on request; finalize at unit start) · Depends on: Unit 07 (documented public HTTP API), Unit 10 (status surface)

## Goal

Ship thin, documented client SDKs so existing applications integrate Deltu
without adopting its stack (architecture: SDK-first; ai-workflow: SDKs
communicate, never reimplement). Both SDKs target the public API only — no
processing logic, no second engine (invariant 15).

## Design

### Layout

```text
sdk/python/       # pip-installable "deltu" package
sdk/typescript/   # npm-installable "@deltu/client" package
```

### Surface (identical shape in both languages)

* `DeltuClient(url, api_key=None)` with:
  * `send_events(events: list[Event]) -> BatchResult` — `POST /v1/events`
  * `send_event(event) -> EventResult` — single-event convenience
  * `health() -> bool`, `status() -> StatusSnapshot`
* `Event` builder validating locally what spec 02 validates server-side
  (same rules, same error messages — cheap pre-flight, not a second
  validator of record).
* Errors: typed exception hierarchy mirroring the HTTP envelope (400/413/
  429/503 preserved as distinct types); no silent retries by default,
  configurable retry with backoff for 429/503 only.
* Types generated/hand-maintained from the Unit 07 documented shapes;
  `StatusSnapshot` mirrors the Unit 10 export field-for-field.
* Both SDKs: zero runtime dependencies beyond an HTTP client
  (`httpx` / `fetch`-based), Apache-2.0 headers, README with a runnable
  example against `deltu run`.

### Release discipline

Versioned in lockstep with the engine's documented API version (`v1`);
`CHANGELOG.md` per SDK; CI jobs running each SDK's tests against a live
`deltu run` instance started by the workflow (the first true end-to-end
coverage of the public contract).

## Implementation

1. Python package (typed, `pydantic`-free — dataclasses suffice), tests
   (`pytest`), example.
2. TypeScript package (strict mode, no `any` per code standards), tests
   (`vitest`), example.
3. CI wiring + live end-to-end job.
4. Scope guard: no streaming/websockets (not in the API yet), no retry
   storms, no auth beyond static API key, no dashboard.

## Dependencies

Runtime: `httpx` (Python), built-in `fetch` (TypeScript). Dev: pytest,
vitest. Nothing else.

## Verify When Done

* [ ] Both packages install, examples run against a live engine.
* [ ] SDK test suites + live e2e CI job green.
* [ ] Public API coverage matches the Unit 07 documented surface; tracker
      updated.
