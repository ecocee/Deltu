# Spec 01 — Rust Foundation

Status: COMPLETE (implemented & verified 2026-09-25, toolchain Rust 1.97.1, edition 2024) · Depends on: Unit 00 (context & research)

## Goal

Initialize the smallest working Rust package for Deltu and verify the full
toolchain workflow (`cargo check`, `cargo build`, `cargo test`, `cargo run`,
`cargo fmt`, `cargo clippy`). This unit does **not** implement the event
engine.

## Design

* One Cargo package at the repository root (created with `cargo init`).
* Library + thin binary: `src/lib.rs` owns the core boundary; `src/main.rs`
  is a minimal executable that calls into the library. This keeps logic
  unit-testable and the executable replaceable.
* Package name: `deltu` (binary name and CLI naming question tracked in
  `progress-tracker.md` → Open Questions; does not block this unit).
* Stable toolchain, default profile, no dependencies. No async runtime
  (decision 004). No HTTP/MQTT/serde yet — those arrive with their units.
* Edition: the current Cargo default (2024 edition as of the 2026 toolchain),
  whatever `cargo init` selects — not hand-pinned.

## Implementation

1. Run `cargo init` at the repository root (package `deltu`).
2. Keep the generated `src/main.rs` "Hello, world!" minimal; add
   `src/lib.rs` exposing the tiny core boundary:

   ```rust
   // src/lib.rs
   //! Deltu core engine — foundation boundary.

   /// Returns the engine version from Cargo metadata.
   pub fn version() -> &'static str {
       env!("CARGO_PKG_VERSION")
   }

   #[cfg(test)]
   mod tests {
       #[test]
       fn version_is_set() {
           assert!(!super::version().is_empty());
       }
   }
   ```

3. `src/main.rs` prints the engine name and version via the library:

   ```rust
   fn main() {
       println!("deltu {}", deltu::version());
   }
   ```

4. Error/result conventions (established now, reused by every later unit):
   * Recoverable failures return `Result<T, E>`; `E` is a project-local
     error enum per module, converted with `From` impls.
   * No `unwrap()`/`expect()` in production paths; tests may use them.
   * No silently discarded errors; no fallback behavior that fakes success.
5. Verify the full checklist below. Do not add dependencies, modules, or
   features beyond this.

## Dependencies

* None (no crates). Stable Rust toolchain only.

## Verify When Done

* [ ] `cargo check` passes with no warnings.
* [ ] `cargo build` succeeds.
* [ ] `cargo test` passes (at least the foundation test).
* [ ] `cargo run` prints the engine name and version.
* [ ] `cargo fmt --check` passes.
* [ ] `cargo clippy` passes with no warnings.
* [ ] `Cargo.toml` contains no dependencies.
* [ ] `context/progress-tracker.md` updated (Unit 01 COMPLETE with results).
