# Topic — Edge

## Question

What does Deltu require to run as a single lightweight binary on ARM64
devices (Raspberry Pi) and x86_64 Linux — including cross compilation from a
development machine?

## Findings

* **Targets**: primary targets are `x86_64-unknown-linux-gnu` and
  `aarch64-unknown-linux-gnu` (Raspberry Pi 4/5 in 64-bit mode, plus ARM64
  servers and containers). Pure-Rust dependencies compile for both without
  special effort.
* **Cross compilation**: the practical toolchains are (1) `cross` (Docker-
  based, zero local sysroot setup) and (2) direct `cargo build
  --target aarch64-unknown-linux-gnu` with an `aarch64-linux-gnu` linker
  and, for GNU targets, the target sysroot available. Docker-based cross
  builds are the lowest-friction repeatable option; a build-stage multi-arch
  Dockerfile is an alternative.
* **C dependencies are the main edge risk**: any C-backed crate (e.g., ONNX
  Runtime, llama.cpp) needs the native library for the target architecture.
  Pure-Rust choices (e.g., `rumqttc` over C MQTT clients) avoid this class
  of problem for core units.
* **Resource constraints**: Raspberry Pi 4/5 class devices (1–8 GB RAM) are
  the reference: a memory-first engine with bounded queues and state fits
  comfortably if capacities are configured, but defaults must assume the
  small end. CPU budget: a single core should sustain meaningful throughput
  for the MVP pipeline; measurements, not assumptions.
* **Local processing / offline operation**: the deterministic core (MQTT/
  HTTP in, state, rules, actions out) runs fully offline. Only AI and
  cloud features require connectivity, and they are optional (invariants
  3, 20).
* **musl**: static musl builds (`*-unknown-linux-musl`) are possible for
  fully-static deployment images; keep as an option, not a requirement.

## Sources

* Rust Embedded Book (targets, cross compilation) — https://docs.rust-embedded.org/book/
* cross project — https://github.com/cross-rs/cross
* cargo book — target selection — https://doc.rust-lang.org/cargo/reference/config.html#target
* Raspberry Pi OS (64-bit) — https://www.raspberrypi.com/software/operating-systems/

## Impact on Deltu

* Dependency rule adds a standing question: "does it affect ARM64 support?"
  (build plan §19) — pure-Rust dependencies are strongly preferred for the
  core.
* CI/build documentation must include at least one repeatable ARM64 path
  (Docker cross build) once the engine is functional.
* Default capacities (queue depth, state entries, dedup cache) must be
  chosen conservatively and be configurable, so a Pi-class device is safe
  out of the box.

## Decision

Deltu targets Linux x86_64 and ARM64 as first-class platforms, prefers
pure-Rust dependencies to avoid cross-compilation risk, uses Docker-based
cross builds for ARM64 verification, and keeps the deterministic core fully
offline-capable with conservative configurable resource defaults.
