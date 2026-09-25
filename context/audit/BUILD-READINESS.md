# Deltu Build & Developer Readiness (Audit 2026-09-26; updated for v0.1.0 release readiness)

## Verdict

**A new developer can clone, build, run, and build something useful with
Deltu today.** The full check battery passes, the release binary works
end-to-end, the CLI lifecycle (`up/status/logs/down`) works, and both
e2e input paths (HTTP and MQTT, verified against a real broker) drive
events through pipeline → state → rules → actions — including the
**webhook output action** added for v0.1.0 and verified against a live
HTTP receiver.

## v0.1.0 release-readiness additions (2026-09-26)

* Webhook action implemented behind the existing executor seam; live
  e2e: rule fired → JSON delivered to a real receiver with configured
  headers; dead receiver → counted failure, engine healthy.
* Runtime action dispatch moved to the blocking pool (`spawn_blocking`)
  — reqwest's blocking client cannot run inside a tokio worker (panic
  found and fixed during verification).
* aarch64 release build verified end-to-end on Apple Silicon: binary
  builds (6.5 MB), starts, `check`/`health`/`run` work, ingest lands in
  state, graceful shutdown clean, RSS ≈ 8.7 MB.
* Full battery re-run after changes: 182 Rust tests, clippy 0, fmt
  clean, SDK suites 12/12 each, MQTT e2e with a real broker, demo
  script showing `actions_fired: 2` (log + webhook).

## What was actually verified in this audit

| Check | Result |
| --- | --- |
| `cargo check --all-targets` | PASS |
| `cargo test` | PASS — 182 tests, 0 failed (170 at audit + 12 release additions) |
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets` | PASS — 0 warnings |
| `cargo build --release` + `strip` | PASS — 5.2 MB binary |
| `deltu --version` / `--help` | PASS |
| `deltu check` valid / invalid configs | PASS (bind validation added in audit) |
| `deltu run` + HTTP ingest → rule → action | PASS |
| `deltu up` / `status` / `logs` / `down` lifecycle | PASS (commands added in audit) |
| Duplicate / invalid / oversize input handling | PASS (dedup counts, 400/413 envelopes) |
| 2000-event burst | PASS — ~118k ev/s, queue drained |
| Sustained 10k-event load | PASS — 47k ev/s, ~10 MB RSS, 0.0% idle CPU after |
| MQTT against real mosquitto (Docker) | PASS — publish → state → rule fired (after audit fix) |
| MQTT broker-down resilience | PASS after fix — 5 reconnects/3 s, 0% CPU |
| SIGTERM shutdown (with and without MQTT) | PASS — 18 ms exit (MQTT hang fixed) |
| Webhook delivery to a live receiver | PASS — JSON body + custom headers verified at the receiver |
| Webhook failure isolation (dead receiver) | PASS — failure counted, engine healthy |
| aarch64 release binary run (Apple Silicon) | PASS — version/check/health/run/ingest/shutdown |
| Restart with snapshot restore | PASS — `restored: 1` |
| Python SDK suite | PASS — 12/12 incl. live engine |
| TypeScript SDK suite | PASS — 12/12 incl. live engine |
| Secrets / stubs scan | PASS — clean |

## Fresh-machine experience (what a new developer hits)

1. **Install/build**: `cargo build --release` — plain Rust toolchain,
   pure-Rust dependency tree, no system libraries. No surprises found.
2. **Start**: `deltu run` (defaults bind loopback:8080) or
   `deltu up --config engine.yaml` for background + logs + `deltu down`.
3. **Configure**: YAML, validated up-front (`deltu check`), with an
   effective-config printer (`deltu config`). The YAML tag syntax for
   rules (`on: !Event`) is unusual but documented in deploy.md, README,
   demo config, and validated with actionable errors.
4. **Send data**: curl/SDK; errors are enveloped and name the offending
   item (`events[0] failed validation: ...`).
5. **Create a rule → trigger action**: documented with working configs
   in `docs/deploy.md`, `docker/engine.yaml`, `examples/demo.sh`.
6. **Inspect**: `deltu status` (JSON counters), `deltu logs`,
   structured action log lines.
7. **Understand what happened**: counters show every drop/dedup/
   suppression reason; README architecture map matches the code.

## Documentation accuracy

README, deploy.md, and specs were cross-checked against behavior during
this audit; the audit fixed the one place the docs described an
implementation that did not exist (MQTT payload contract). The
windowing semantics (trailing window stays open; summaries fire on
advance) and state expiry behavior are now documented in code, specs,
and USE-CASES.md.

## Gaps that matter (not build blockers)

See KNOWN-ISSUES.md. Headline: no webhook output action, no HTTP auth,
release workflow unexercised until first tag. All are scoped and
documented; none prevents a developer from building something useful.

## Known environment caveats from the audit

- Docker daemon was not running locally during Units 13–14; the
  Dockerfile is verified by the CI smoke workflow, and the MQTT broker
  path was verified in this audit via Docker.
- Linux ARM64 (Pi) is cross-compiled in CI but was not physically
  tested on a device.
