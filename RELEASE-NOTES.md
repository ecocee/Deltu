# DELTU v0.1.0 — Make Data Behave.

DELTU is a lightweight, local-first event-processing engine built in Rust.

It turns continuous data into meaningful events, state, rules, and actions while keeping resource usage low.

## Highlights

- HTTP event ingestion (`POST /v1/events`, typed error envelope, backpressure)
- MQTT ingestion (MQTT 5 / 3.1.1, reconnect backoff, bounded buffering)
- Filtering (kind allow-list)
- Deduplication (bounded id cache, FIFO eviction)
- Aggregation (tumbling windows: mean/min/max/last)
- Change detection (deadband suppression)
- Bounded state (capacity-capped, periodic expiry, never unbounded)
- Stateful rules (event/state triggers, typed operators, suppression windows)
- Log actions (structured JSON lines)
- Webhook actions (configurable URL/method/headers/timeout, failure-isolated)
- Python SDK (`deltu` on pip-installable source)
- TypeScript SDK (`@deltu/client`, zero runtime dependencies)
- CLI tooling (`run` / `check` / `status` / `health` / `up` / `down` / `logs` / `config`)
- Docker deployment (distroless, non-root, healthcheck)
- ARM64 support (macOS ARM64 verified; Linux ARM64 via cross-compilation)
- No mandatory cloud services
- No telemetry
- AI-optional architecture (provider boundary, policy-enforced, excluded from the default build)

## Verification

- 182 Rust tests (`cargo test --lib`)
- 24 SDK tests (12 Python + 12 TypeScript, incl. live-engine end-to-end)
- HTTP E2E verified (ingest → processing → state → rule → webhook)
- MQTT E2E verified (against a real broker)
- Webhook E2E verified (delivery body/headers asserted at a live receiver; failure isolation proven)
- CLI verified (`run` / `check` / `status` / `health` / `up` / `down` / `logs` / `config`)
- Docker verified (image builds; MQTT compose path exercised)
- macOS ARM64 execution verified (release binary run end-to-end)

## Known limitations

- No engine-side authentication — bind loopback (default) or front with an authenticating reverse proxy
- MQTT is inbound only (publish action is future work)
- Windows binaries are unsupported
- Raspberry Pi hardware test remains future verification (aarch64 codegen verified on Apple Silicon)
- GitHub Actions release workflow must be verified during the actual release

## Install

Download the artifact for your platform from this release, verify with
`SHA256SUMS`, `chmod +x deltu`, then `./deltu run`. Full guide:
[`docs/deploy.md`](docs/deploy.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE), and
[`TRADEMARKS.md`](TRADEMARKS.md).
