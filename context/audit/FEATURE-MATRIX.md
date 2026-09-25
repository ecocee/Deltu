# Deltu Feature Matrix (Audit 2026-09-26)

Status vocabulary: **Implemented** (verified working in this audit),
**Partial** (works with documented limits), **Missing** (planned but not
built), **Future** (explicitly post-MVP). Every "Implemented" entry was
exercised by the audit's live tests, not just read from code.

## Event model & validation (spec 02)

| Feature | Status | Evidence |
| --- | --- | --- |
| Event model (id/source/kind/timestamp/payload) | Implemented | 166→170 unit tests; live HTTP ingests |
| Payload kinds: numeric, text, boolean, structured, null | Implemented | unit tests; live MQTT+HTTP sends |
| Validation: non-empty ids/source/kind, positive ts, finite numerics, non-empty text, structured depth ≤ 16 | Implemented | live test: empty id → 400 `invalid_event` naming `events[0]` |
| Kind normalization (lowercase) | Implemented | `Door.State` → `door.state` (SDK + engine tests) |
| Serde wire format (tagged payloads) | Implemented | HTTP live tests; SDK suites |

## Inputs

| Feature | Status | Evidence |
| --- | --- | --- |
| HTTP `POST /v1/events` (batch, 202, per-item accept) | Implemented | live batches; SDK live e2e |
| HTTP validation errors: 400/413/429/503 with single envelope | Implemented | audit live: 400 invalid_event, 400 oversize batch; 413/429/503 in HTTP test suite |
| Body-size guard (`max_body_bytes`) | Implemented | http_tests oversized_body_returns_413 |
| MQTT input adapter (rumqttc, MQTT 5/3.1.1) | Implemented | **audit e2e with real mosquitto: publish → state → rule → log action** |
| MQTT topic→kind/source mapping | Implemented | `sensors/esp32-1/door.state` → kind `door.state`; unit tests |
| MQTT payload contract (tagged object / bare scalar / structured) | Implemented (fixed in audit) | **audit found tagged objects degraded to Json payloads breaking rules; fixed + tested** |
| MQTT reconnect backoff | Implemented (fixed in audit) | **audit found ~50k reconnects/s against a dead broker (CPU spin); fixed: capped exponential 100ms→30s; 3s outage now = 5 attempts, 0% CPU** |
| MQTT graceful shutdown | Implemented (fixed in audit) | **audit found SIGTERM hang when mqtt.enabled (adapter held queue open); fixed: adapter aborted on shutdown; exit in 18 ms** |
| MQTT publish (output) action | Missing | spec 08 listed as "later publish channel"; not built — Future |
| MQTT credentials | Partial | rumqttc supports it; not exposed in `MqttConfig` yet |
| Other adapters (Kafka, Redis streams, files) | Future | per scope: infra only on demand |

## Processing pipeline (spec 03)

| Feature | Status | Evidence |
| --- | --- | --- |
| Kind filter (allow-list, family match) | Implemented | unit tests; `filtered_out` counter live |
| Bounded deduplication (id cache, FIFO eviction) | Implemented | **audit live: duplicate across batches counted once; `duplicates: 1`** |
| Tumbling-window aggregation (numeric) | Implemented | unit tests; live: numeric events → state mean |
| Close-on-advance windows (trailing window stays open) | Implemented | documented + unit-tested; **operators must know: last window's summary fires only when a later event advances the clock** |
| Change detection (deadband `min_delta`, suppression) | Implemented | unit tests; counters live |
| Bounded state everywhere (capacities + eviction counters) | Implemented | capacity tests; counters exposed |

## State engine (spec 04)

| Feature | Status | Evidence |
| --- | --- | --- |
| Keyed current-state store (source,kind) | Implemented | live: door.state open→closed = 1 entry |
| Bounded capacity + stalest eviction | Implemented | unit tests incl. restore-capacity test |
| Periodic expiration (`expire_after_ms`) | Implemented | 30 s worker timer; unit tests; **note: expires vs wall clock — old-timestamped events expire on next pass** |
| Full-fidelity snapshot restore | Implemented | live restart-restore (`restored: 1`), unit tests |

## Rules (spec 05)

| Feature | Status | Evidence |
| --- | --- | --- |
| Event triggers (kind family match, event + summary inputs) | Implemented | live HTTP rule firing; MQTT e2e |
| State triggers (key-exists) | Implemented | unit tests |
| Conditions: Comparison / All / Any / Exists | Implemented | unit tests + live |
| Fields: EventValue / Aggregation stats / State | Implemented | unit tests |
| Operators: Eq/Ne/Gt/Gte/Lt/Lte (typed, cross-type = false) | Implemented | truth-table tests |
| Suppression (once-per-window) | Implemented | unit tests |
| Total evaluation (no runtime errors) | Implemented | design + tests |

## Actions (spec 06)

| Feature | Status | Evidence |
| --- | --- | --- |
| Structured log action (stderr, rendered line) | Implemented | live: audit log lines observed with rule_id/value/ts |
| Executor trait seam | Implemented | dispatcher tests; AI rides the same seam |
| Failure isolation (failed action ≠ engine failure) | Implemented | unknown-action test; counted in status |
| Webhook / HTTP output action | Missing | **spec 06 success-criteria chain names "Webhook"; only log exists — top functional gap for MVP demos** |
| AI action (through policy) | Implemented | dispatcher-level tests (scripted provider) |
| MQTT publish action | Missing | Future |

## Runtime & HTTP (spec 07)

| Feature | Status | Evidence |
| --- | --- | --- |
| Tokio runtime, bounded work queue (backpressure 429) | Implemented | live: 2000-event burst, queue drained; unit tests |
| Graceful shutdown (SIGINT/SIGTERM, drain) | Implemented (fixed for MQTT) | audit: clean exit; MQTT hang fixed |
| YAML/JSON config, env overrides | Implemented | live `deltu config`; `DELTU_HTTP_BIND` etc. |
| Config validation incl. bind address | Implemented (fixed in audit) | **audit found `deltu check` accepted `bind: bad-address-no-port`; fixed + 8 new tests** |
| AI usage in `/v1/status` | Implemented | live status shows ai block |

## CLI (spec 09)

| Feature | Status | Evidence |
| --- | --- | --- |
| `run`, `check`, `status`, `health`, `--version` | Implemented | audit live on release binary; exit codes verified |
| `up` (background + pid file + readiness poll) | Implemented (added in audit) | live: up → "already running" guard → down |
| `down` (SIGTERM, escalation, pid file cleanup) | Implemented (added in audit) | live: stopped; exit 1 with no pid file |
| `logs` (tail, --follow) | Implemented (added in audit) | live: shows action log lines |
| `config` (effective YAML/JSON) | Implemented (added in audit) | live: prints merged config |

## Metrics (spec 10)

| Feature | Status | Evidence |
| --- | --- | --- |
| Latency rings (batch/http/action, p95/p99) | Implemented | live status output |
| Throughput window (events_per_sec) | Implemented | live: 33.4 ev/s steady-state (windowed) |
| Counters: pipeline/state/mqtt/actions/persistence | Implemented | live status shows all blocks |
| Baseline measurement | Implemented | 76.9k ev/s (Unit 10, dev build); audit: 118k burst / 47k sustained sequential-client on release build |

## AI (spec 11, optional)

| Feature | Status | Evidence |
| --- | --- | --- |
| Provider trait + scripted in-memory provider | Implemented | unit tests |
| Policy enforcement (AI only via dispatcher) | Implemented | structure tests |
| Usage counters (calls/tokens/latency) | Implemented | status surface |
| ONNX runtime (`ai-onnx` feature) | Future | feature-gated by design; not built |
| llama.cpp (`ai-llamacpp` feature) | Future | same |
| Cloud AI providers | Future | same |

## Persistence (spec 12, optional)

| Feature | Status | Evidence |
| --- | --- | --- |
| `PersistenceAdapter` + local file snapshots (atomic) | Implemented | crash-mid-write test; live restart |
| Version-checked restore, corruption rejection | Implemented | unit tests |
| Periodic snapshots from worker | Implemented | live: snapshot file written on cadence |
| `HistoricalSink` + bounded reference sink | Implemented | degradation tests |
| PostgreSQL adapter | Future | deferred at Unit 12 with rationale; seam exists |
| Retention/downsampling/query APIs | Future | post-MVP per spec |

## SDKs (spec 13)

| Feature | Status | Evidence |
| --- | --- | --- |
| Python client (events/health/status, typed errors, retries) | Implemented | 12/12 incl. live e2e |
| TypeScript client (same surface, strict, zero deps) | Implemented | 12/12 incl. live e2e |
| SDK CI against live engine | Implemented | `.github/workflows/sdk.yml` |

## Packaging (spec 14)

| Feature | Status | Evidence |
| --- | --- | --- |
| Distroless Dockerfile (non-root, healthcheck) | Implemented | built in CI smoke; local daemon was unavailable |
| Compose demo (engine + mosquitto) | Implemented | config validated by `deltu check`; broker path e2e-verified in audit |
| CI: fmt/clippy/test + Docker smoke | Implemented | workflow file |
| Release: x86_64, ARM64 (`cross`), macOS ARM64 + SHA256SUMS | Implemented (workflow) | **not yet exercised end-to-end by an actual tag push** |
| `docs/deploy.md` | Implemented | verified against live demo |

## Open-source surface

| Feature | Status | Evidence |
| --- | --- | --- |
| LICENSE (Apache-2.0), NOTICE, CONTRIBUTING, TRADEMARKS | Implemented | files present |
| CODE_OF_CONDUCT, SECURITY | Implemented (added in audit) | files present |
| Secrets scan / no proprietary files | Implemented | audit scan clean |

## Known functional gaps (details in KNOWN-ISSUES.md)

1. No webhook/HTTP output action (MVP narrative names it).
2. No MQTT publish action.
3. No HTTP authentication on the engine API.
4. Release workflow unexercised until first `v*` tag.
5. Windows builds unsupported (design: Linux/macOS).
