# Spec 13 — SDKs (Python, TypeScript)

Status: COMPLETE (implemented & verified 2026-09-25; finalized semantics below) · Depends on: Unit 07 (documented public HTTP API), Unit 10 (status surface)

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

* [x] Both packages install, examples run against a live engine.
* [x] SDK test suites + live e2e CI job green.
* [x] Public API coverage matches the Unit 07 documented surface; tracker
      updated.

## Finalized at Unit Start (review pass, 2026-09-25)

1. **Local validation is a pre-flight only** mirroring spec 02 rules with
   the engine's message text; the engine remains the validator of record
   and its errors are surfaced verbatim (SDK never re-decides).
2. **Error mapping covers exactly the documented codes** (400/413/429/503
   → four distinct types); any other non-2xx surfaces as the base
   `DeltuError` carrying the envelope's `code` — no guessing, no
   swallow-and-retry.
3. **Retries are off by default**, exponential backoff, 429/503 only —
   matching the engine's own `queue_full` advice; 400/413 are never
   retried.
4. **Payload typing mirrors the wire**: Python tagged dicts with helper
   constructors (`text()`, `numeric()`, `boolean()`, `structured()`,
   `null()`); TypeScript a discriminated union over the same shapes.
5. **`StatusSnapshot` maps snake_case → idiomatic casing** field-for-field
   from spec 10 and keeps the raw body (`raw`) so new engine fields are
   visible without an SDK release.

## Verified (2026-09-25)

* Python: 12/12 tests (incl. live e2e against a real `deltu run` on
  127.0.0.1:8210 — health, send, status asserted against the engine).
* TypeScript: 12/12 tests (same live e2e), strict `tsc` build clean,
  zero runtime dependencies.
* `.github/workflows/sdk.yml` runs both suites against a live engine
  started by CI (first true end-to-end coverage of the public contract;
  repo-wide CI lands with Unit 14).
