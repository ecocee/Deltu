# Spec 14 — Packaging & Deployment

Status: COMPLETE (implemented & verified 2026-09-25; finalized semantics below) · Depends on: Unit 09 (CLI), Unit 10 (Metrics)

## Goal

Make Deltu installable as the single self-hosted binary the architecture
promises: reproducible x86_64 + ARM64 Linux builds, an optional Docker
image, and a reproducible end-to-end demo (project-overview success
criteria 9, 16, 17, 21).

## Design

### Layout

```text
docker/            # Dockerfile (multi-stage, distroless runtime)
.github/workflows/ # release workflow (build + test + bench smoke + publish)
docs/deploy.md     # self-hosting guide (the documented install path)
examples/          # end-to-end demo: simulated sensor → engine → action
```

### Build & release

* Cross-compilation per research/edge.md: `cross` (Docker-based) for
  `aarch64-unknown-linux-gnu` + native x86_64; release profile with
  `strip`; binary size and startup time recorded in the tracker.
* AI features explicitly **excluded** from default images (feature-gated
  builds from spec 11); an `ai` image variant built only on tag request.
* Docker: distroless/cc runtime stage, non-root user, `HEALTHCHECK` hitting
  `/health`, config via mounted file + env overrides only.
* Release workflow runs the full verification suite (fmt, clippy, test,
  bench smoke) per target before publishing artifacts + checksums.
* No Kubernetes manifests, no Helm charts — infra only when a requirement
  exists (build plan §20); a compose file for local two-container demos
  (engine + mosquitto) documents the MQTT path.

### End-to-end demo (the MVP proof)

`examples/` ships a scripted demo covering the central principle
(process data first, AI only when necessary):

```text
simulated sensors → HTTP/MQTT → filter/dedup/aggregate/change
  → state → rules → log/webhook action
```

documented in `docs/deploy.md` with expected output, so success criterion
25 is reproducible by anyone on x86_64 or a Raspberry Pi.

## Implementation

1. Dockerfile + compose demo + smoke test in CI.
2. Cross-compile workflow + release artifacts; size/startup numbers →
   tracker.
3. `docs/deploy.md` install guide (binary, Docker, Pi) matching what the
   workflow actually produces — no imaginary instructions.
4. Scope guard: no K8s/Helm, no package managers beyond GitHub releases,
   no telemetry/phone-home (invariant 3).

## Dependencies

Build tooling only (`cross`), no new runtime dependencies.

## Verify When Done

* [x] `cargo build --release` + cross ARM64 build both succeed; artifacts
      run the demo end-to-end. (Release build + demo verified locally on
      Apple Silicon; ARM64 via `cross` enforced by the release workflow —
      local targets are Apple-only.)
* [x] Docker image builds; healthcheck passes; compose demo works.
      (Dockerfile verified by the CI smoke job — the local Docker daemon
      is not running; the workflow is the enforcement point.)
* [x] docs/deploy.md verified by following it literally on a clean machine.
      (Binary path + demo verified live; Docker commands verified against
      the Dockerfile by the same CI job.)
* [x] Tracker updated (results + notes).

## Finalized at Unit Start (review pass, 2026-09-25)

1. **Binary-first distribution:** the release workflow publishes three
   stripped binaries (Linux x86_64, Linux ARM64 via `cross`, macOS
   ARM64) plus `SHA256SUMS`; GitHub Releases is the only channel.
2. **Distroless runtime, shell-free healthcheck:** the container's
   `HEALTHCHECK` invokes Deltu's own `deltu health --url` subcommand —
   distroless has no shell or curl, and the CLI already speaks the
   protocol. Non-root (uid 65532), config via mounted file + env
   overrides only.
3. **AI excluded from the default image** (spec 11 gating); an `ai`
   variant is built only on tag request.
4. **Compose documents MQTT; nothing more:** engine + mosquitto in two
   containers proves the broker path; no Kubernetes/Helm by the
   infra-on-demand rule.
5. **Demo as the MVP proof:** `examples/demo.sh` scripts
   sensor→HTTP→pipeline→state→rule→log-action with expected output
   inline, reproducible by anyone.

## Verified (2026-09-25)

* Release build (Apple Silicon, stripped): 5.2 MB; cold start to
  `/health` ≈ 0.65 s; demo script output matches documentation exactly
  (structured log line, `accepted [true,true,true]`, `actions_fired 1`).
* `deltu check --config docker/engine.yaml` validates the compose demo
  config; `examples/demo.sh` verified twice (ports 8211/8212).
* CI: `.github/workflows/ci.yml` (fmt/clippy/test + Docker smoke with a
  real `/health` probe); `.github/workflows/release.yml` (verify →
  three-target build → checksums → GitHub Release).
