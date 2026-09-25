# Deltu Final Audit (2026-09-26)

Full-repository audit performed on `feat/15-readme` (containing the
complete 14-unit build plan). Method: read the intended product from
`context/` (overview, build plan, specs, decisions, research), then
verify every claim against actual behavior — compile, test, run the
release binary, drive real end-to-end flows (HTTP and MQTT against a
real broker), abuse the inputs, measure resources, and walk the
new-developer path.

## Final status

```text
DELTU STATUS

Build:            PASS        (cargo check/test/fmt/clippy all clean)
Tests:            PASS        (170 lib tests + 24 SDK tests, 0 failures)
CLI:              PASS        (run/check/status/health + up/down/logs/config)
Core Engine:      READY       (filter/dedup/aggregate/change/state/rules verified)
HTTP:             READY       (e2e verified; envelopes; backpressure; oversize)
MQTT:             READY       (e2e with real broker, after 3 audit fixes)
Rules:            READY       (event+state triggers, typed operators, suppression)
State:            READY       (bounded, expiring, snapshot restore verified)
Actions:          READY*      (log + AI actions isolated; *webhook missing)
SDKs:             READY       (12/12 tests each incl. live engine)
Documentation:    READY       (matches implementation after audit sync)
Open Source:      READY       (LICENSE/NOTICE/CONTRIBUTING/TRADEMARKS/COC/SECURITY)
Examples:         READY       (demo.sh verified live; compose demo validated)
Performance:      VERIFIED    (118k ev/s burst, 47k sustained, ~10 MB RSS)

Overall:
READY FOR PUBLIC USE AS AN MVP — with two pre-release recommendations
(Do a v0.1.0-rc dry run of the release workflow; ship the webhook
action or clearly label the log action as the current output path).
Not "production-hardened": no auth on the API, single-process only.
```

## What the audit found and fixed

Four real bugs, all found by exercising the binary rather than reading
docs (details in KNOWN-ISSUES.md):

1. `deltu check` accepted invalid bind addresses → fixed with
   validation + tests.
2. MQTT reconnect storm (~50k attempts/s, 100% CPU on broker outage) →
   fixed with capped exponential backoff.
3. SIGTERM hang when `mqtt.enabled` (adapter held the queue open) →
   fixed by aborting the adapter on shutdown (18 ms exit now).
4. MQTT tagged payloads (`{"type":"text","value":...}`) degraded to
   structured payloads so rules never matched → fixed per the
   documented contract; e2e with a real broker now fires the rule.

Developer-usability gaps closed: the documented basic lifecycle
(`deltu up / status / logs / down / config`) did not exist — implemented
as thin, honest commands (pid file + readiness poll + tail) and verified
live, including double-`up` idempotency and `down` with no engine.

Open-source completeness: added `CODE_OF_CONDUCT.md` and `SECURITY.md`
(the latter states the auth posture honestly).

## Architecture verdict

The implementation matches the intended architecture with one honest
caveat: numeric events aggregate into windows, so threshold rules fire
on window summaries (firing on advance), while non-numeric events pass
through immediately. This is spec'd behavior, not a defect, but it
surprises newcomers — now documented in three places. No unnecessary
infrastructure exists: no Kafka/Redis/K8s/cloud; dependencies are
minimal and justified; the AI and persistence layers are genuinely
optional (default build excludes AI code entirely).

Dead code / coupling: none material found. The `_config_shape_check`
placeholder and one `#[allow(dead_code)]` helper in `http.rs` are the
only cruft and are harmless.

## Performance verdict (release build, Apple Silicon)

| Measure | Result |
| --- | --- |
| Burst ingest (2000 events, sequential client) | ~118k ev/s |
| Sustained (10k events, sequential client) | ~47k ev/s |
| Peak RSS under load | ~10 MB |
| CPU after load settles | 0.0% |
| Cold start → healthy | ~0.65 s |
| Unit 10 baseline (concurrent driver, dev build) | 76.9k ev/s, p95/p99 = 1 ms |

Sequential-client numbers are client-bound; the engine keeps up with
every workload thrown at it and drains its queue to zero. Memory is
bounded by design and observed.

## Use-case verdict

Nine use cases assessed (USE-CASES.md): sensor/IoT telemetry, device
state tracking, application/API event streams, infra metrics, log
filtering, change detection, edge processing, AI-assisted decisions
(boundary), automated actions (gap). Deltu is **not** IoT-only — the
HTTP API + SDKs make application-event and metrics use cases first-class.
Claims are limited to what was verified.

## Open-source verdict

Apache-2.0 with ECOCEE NOTICE, contributor IP terms that keep ownership
with contributors (CLA deliberately deferred), trademark policy
separating the brand from the code, CoC, security policy with an honest
auth statement, secrets scan clean. Suitable for public GitHub release
after the two pre-release recommendations above are handled (or
accepted as documented limitations).

## Pre-release checklist (recommended order)

1. Dry-run `release.yml` on a `v0.1.0-rc1` tag; fix first-run issues.
2. Ship the webhook output action (small, high demo value) — or adjust
   the criterion-25 narrative to the log action.
3. Decide the auth story for exposed deployments (proxy guidance exists).
4. Physical test on a Raspberry Pi (ARM64 artifact).
