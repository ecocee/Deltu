# Progress Tracker

Update this file after every meaningful implementation change.

## Current Phase

- Unit 03 — Processing Core: **NOT STARTED** (spec to be written first)

## Current Goal

- Write `context/specs/03-processing-core.md` (filter → deduplicate →
  aggregate → change-detect stages per `research/event-processing.md`),
  then implement and verify the pipeline stages.

## Completed

- Unit 00 — Planning, Research & Context: **COMPLETE** (2026-09-25)
  - Context knowledge base, research collection, decision records 001–004,
    and specs 00/01 reviewed and in use by the implemented Unit 01.
- Unit 02 — Event Model: **COMPLETE** (2026-09-25, per
  `context/specs/02-event-model.md`, branch `feat/02-event-model`)
  - `event` module (`mod.rs`, `payload.rs`, `error.rs`): `Event`, closed
    `Payload` enum, `EventError` with actionable Display messages;
    re-exported at the crate root.
  - Validation/normalization per spec: trimmed non-empty id/source/kind,
    lowercased kind, positive epoch-millis timestamp, finite numerics,
    non-empty text; `Event::new` and the adapter path
    (deserialize → `validate` → pipeline) both covered.
  - Dependencies added: `serde` 1.0.229 (derive) + `serde_json` 1.0.151,
    the two justified in the spec; pinned in `Cargo.lock`.
  - Spec refined during implementation: `Payload` uses **struct** variants
    (`Numeric { value: f64 }`) because serde's internally tagged
    representation cannot serialize primitive newtype variants; the
    documented JSON contract is unchanged.
  - Verification results: `cargo check` clean; `cargo build` ok;
    `cargo test` 24 passed / 0 failed (foundation test still green);
    `cargo run` → `deltu 0.1.0`; `cargo fmt --check` clean;
    `cargo clippy` 0 warnings. All spec checklist items pass.
- Unit 01 — Rust Foundation: **COMPLETE** (2026-09-25, per
  `context/specs/01-rust-foundation.md`)
  - `cargo init --name deltu` at repository root; edition 2024; zero
    dependencies in `Cargo.toml`.
  - `src/lib.rs` minimal core boundary (`deltu::version()`);
    `src/main.rs` prints `deltu 0.1.0` via the library.
  - Error/result conventions established in the spec (local error enums,
    `From` conversions, no `unwrap()` in production paths) for reuse by
    later units.
  - Verification results: `cargo check` clean (0 warnings); `cargo build`
    ok; `cargo test` 1 passed / 0 failed; `cargo run` → `deltu 0.1.0`;
    `cargo fmt --check` clean; `cargo clippy` 0 warnings. All spec checklist
    items pass.

## In Progress

- None.

## Next Up

- Unit 03 — Processing Core: write `context/specs/03-processing-core.md`
  (filter → deduplicate → aggregate → change-detect stages per
  `research/event-processing.md`), then implement and verify it.

## Open Questions

- CLI binary name: architecture and build-plan examples use `plan-c` while the
  project is named Deltu. The final binary name must be confirmed before the CLI
  unit (Unit 09). Does not block earlier units.
- MQTT protocol version default for the first MQTT unit: 3.1.1 vs 5.0 (research
  suggests 5.0-capable client with a configured default; decide in that unit's spec).

## Architecture Decisions

- 001 — Rust core engine, single self-hosted binary (`context/decisions/001-rust-core.md`).
- 002 — Memory-first runtime state; persistence stays optional
  (`context/decisions/002-memory-first-state.md`).
- 003 — AI is optional and isolated behind a provider abstraction
  (`context/decisions/003-ai-optional.md`).
- 004 — Async runtime deferred until continuous processing requires it
  (`context/decisions/004-deferred-async-runtime.md`).

## Session Notes

- Repository layout: `Cargo.toml` (package `deltu` 0.1.0, edition 2024,
  no dependencies), `Cargo.lock`, `src/lib.rs`, `src/main.rs`, `target/`
  (ignored), plus the original `context/` and `AGENT.md`.
- Unit 01 was implemented strictly within spec scope: no dependencies, no
  async runtime (decision 004), no event-engine code. The next unit begins
  with its spec, per the workflow rules.
- Git: `feat/01-rust-foundation` holds Units 00–01 (commits `2f06984`,
  `277aac3`) and is synced to origin; Unit 02 lives on
  `feat/02-event-model`. Direct pushes from the coding shell lack HTTPS
  credentials; sync via the client or a credentialed environment.
