# Progress Tracker

Update this file after every meaningful implementation change.

## Current Phase

- Unit 02 — Event Model: **NOT STARTED** (spec to be written first)

## Current Goal

- Write `context/specs/02-event-model.md`, then implement and verify the
  validated internal event model.

## Completed

- Unit 00 — Planning, Research & Context: **COMPLETE** (2026-09-25)
  - Context knowledge base, research collection, decision records 001–004,
    and specs 00/01 reviewed and in use by the implemented Unit 01.
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

- Unit 02 — Event Model: write `context/specs/02-event-model.md`
  (validated, serializable internal event struct per
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
- Git: repository initialized 2026-09-25; branch `feat/01-rust-foundation`
  holds Units 00–01. Push to GitHub pending a remote (no `gh` CLI and no
  origin configured in this environment; see commit step notes).
