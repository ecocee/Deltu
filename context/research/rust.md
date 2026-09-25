# Topic — Rust

## Question

How should the Deltu core engine be structured, initialized, and evolved in
Rust so that it stays small, testable, and self-hostable?

## Findings

* **Cargo** is the Rust package manager and build tool. `cargo init` creates a
  package in an existing directory; `cargo new` creates a new directory. The
  standard workflow is `cargo check` (fast validation), `cargo build`,
  `cargo test`, and `cargo run`.
* **Project layout**: a package can contain a binary target (`src/main.rs`),
  a library target (`src/lib.rs`), and integration tests (`tests/`). A
  library + thin binary split keeps logic testable and reusable while the
  executable stays minimal.
* **Modules**: `mod` declarations plus file-per-module organization give clear
  ownership boundaries. Visibility is explicit (`pub(crate)` for internal
  APIs). No runtime plugin machinery is needed at this stage.
* **Error handling**: `Result` is the idiomatic recoverable-error channel.
  A small project-local error enum avoids an error-handling framework while
  the surface is tiny. Conversion via `From` allows `?` propagation without
  extra crates.
* **Testing**: `#[cfg(test)]` unit tests live next to the code; integration
  tests go in `tests/`. `cargo test` runs both. This is sufficient to start;
  property tests and benchmarks are added later with `proptest`/`criterion`
  when justified.
* **Stable Rust**: stable toolchain only (1.98.x in September 2026, released
  every six weeks). No nightly features are required for the core engine.
* **Dependencies**: every crate is evaluated against the dependency rule
  (build plan §19): problem solved, stdlib alternative, runtime cost, ARM64
  impact, maintenance, coupling.

## Sources

* The Rust Programming Language — https://doc.rust-lang.org/book/
* The Cargo Book (`cargo init`, project layout, commands) —
  https://doc.rust-lang.org/cargo/
* Rust API Guidelines — https://rust-lang.github.io/api-guidelines/
* std::error / Result documentation — https://doc.rust-lang.org/std/result/
* Rust release cadence — https://releases.rs/ and https://blog.rust-lang.org/

## Impact on Deltu

* Unit 01 initializes the package with `cargo init` at the repository root and
  keeps `src/main.rs` minimal behind a small `src/lib.rs` core boundary.
* Error/result conventions are defined once in the foundation spec and reused
  by every later module (event, processing, state, rules, actions).
* No async runtime is introduced until continuous processing actually requires
  it (see `decisions/004-deferred-async-runtime.md`).
* Formatting (`cargo fmt`) and linting (`cargo clippy`) are part of the
  verification checklist from the first unit onward.

## Decision

Deltu uses stable Rust with Cargo defaults: one package, lib + bin split,
project-local error enums, unit tests alongside code, integration tests in
`tests/`, and `cargo fmt --check` + `cargo clippy` in the definition of done.
Dependencies are added only when the standard library cannot reasonably solve
the problem.
