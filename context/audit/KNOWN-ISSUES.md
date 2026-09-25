# Deltu Known Issues (Audit 2026-09-26; updated for v0.1.0 release readiness)

Issues found by the audit, with severity, status, and disposition.
"Bugs fixed in audit" are closed — listed because they shipped in
commits and reviewers should know the history.

## Bugs fixed during this audit

1. **`deltu check` accepted an invalid bind address** (e.g. missing
   port) and failed only at `run` time — deploy-time validation was
   incomplete. *Fixed:* syntactic `host:port` validation in
   `RuntimeConfig::validate`, applied to file values and env overrides;
   2 new tests (168 total at fix time).
2. **MQTT reconnect storm**: with an unreachable broker the adapter
   retried immediately (~149k reconnects in 3 s, ~100% of a CPU core)
   because rumqttc 0.25.1 performs no backoff between `poll()` retries.
   *Fixed:* adapter-owned capped exponential backoff (100 ms → 30 s,
   reset on ConnAck). Verified: 5 reconnects in 3 s, 0% CPU.
3. **SIGTERM hang with MQTT enabled**: the adapter task held a work-
   queue sender clone, so the worker's channel never closed and
   graceful shutdown blocked forever. *Fixed:* the runtime aborts the
   adapter before draining. Verified: 18 ms SIGTERM-to-exit.
4. **MQTT tagged-payload contract not honored**: payloads shaped like
   the HTTP API's (`{"type":"text","value":"open"}`) degraded to
   structured (Json) payloads, so `EventValue` rules never matched —
   the MQTT → rule → action chain silently no-oped for the documented
   payload shape. *Fixed:* conversion now unwraps the documented
   `value` envelope (and the module doc finally matches the code);
   2 new tests. Verified end-to-end with a real broker.

## Open — functional gaps (prioritized; updated 2026-09-26)

1. ~~**No webhook/HTTP output action**~~ **CLOSED (v0.1.0):**
   `ActionKind::Webhook` shipped — configurable URL, method
   (POST/PUT/PATCH), headers, timeout (1..=30000 ms); one bounded
   request per firing, no retries; failures counted, engine continues.
   Verified with 10 tests and a live e2e (rule → webhook → receiver
   asserting body and headers).
2. **No engine-side authentication** (HIGH if exposed beyond
   localhost). The API is unauthenticated; SDKs accept an `api_key` but
   the engine ignores it. Mitigation: bind loopback (default) and front
   with an authenticating proxy — the deployment boundary is documented
   in `docs/deploy.md` §Security boundary and `SECURITY.md`.
3. **No MQTT publish (output) action** (MEDIUM). Inbound-only MQTT
   today; the compose demo's "publish" path is aspirational until the
   action exists.
4. **Trailing aggregation window is only visible on advance** (MEDIUM,
   semantics, by design). A numeric event's window summary (and any
   rule on it) fires when a *later* event advances the window clock —
   a real-time system with one final event will not see that event's
   summary until the next arrives. Operators should window with traffic
   in mind or send a boundary event. Candidate future work: a runtime
   wall-clock flush timer.
5. **State-expiry uses wall clock vs event timestamps** (LOW). Events
   with old timestamps expire on the next periodic pass after restore.
   Correct-by-design for stale-data hygiene, but surprising for
   replay-style workloads; documented in spec 12.
6. **No periodic (state-trigger) rule scheduler** (MEDIUM). State-
   trigger rules are candidates only on inputs; "went offline" patterns
   need periodic evaluation the runtime does not schedule yet (spec 05
   notes this; the EXPIRE_INTERVAL timer is the natural home).

## Open — release-engineering gaps

7. **Release workflow unexercised on GitHub** (MEDIUM).
   `release.yml` (3-target build, checksums, GitHub Release) has never
   run against a real tag — GitHub Actions cannot be executed from this
   environment. Locally verified instead: the aarch64 release build
   compiles clean and the resulting ARM64 binary was executed
   end-to-end (health, ingest, status, graceful shutdown); Linux
   targets use the same dependency set. Do a `v0.1.0-rc1` tag dry run
   before the public release to exercise `cross` and the checksum job.
8. **Docker image unverified locally** (LOW). The local Docker daemon
   was unavailable during Units 13–14; verification rides on the CI
   smoke workflow. The mosquitto container *was* used in this audit,
   so Docker-based e2e works on this machine when the daemon runs.
9. **Windows builds unsupported** (LOW, by design). Documented; not a
   defect. Revisit only on demand.

## Open — minor

10. `deltu up`/`down`/`logs` default paths live in `/tmp` (convenience
    only; production should use a supervisor). Documented in `--help`.
11. Python SDK requires Python ≥3.9; tested on 3.9 (floor) — newer
    versions untested locally.
12. The README claims Pi-class deployment via ARM64 artifacts; no
    physical device test has occurred (honest-label: target claim).

## Secrets / privacy

Scan for credentials, private keys, and customer data: **clean**. No
proprietary files present; third-party notices handled via NOTICE +
crate metadata.
