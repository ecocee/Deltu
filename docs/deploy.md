# Self-hosting Deltu

Deltu ships as a **single static binary** (plus an optional Docker
image). This guide documents the install paths the release workflow
actually produces — nothing here is aspirational.

## 1. Binary (recommended)

Download the artifact for your platform from the latest GitHub release
(`v0.1.0+`), verify the checksum, and run it:

| Artifact | Platform |
| --- | --- |
| `deltu-x86_64-unknown-linux-gnu` | Linux x86_64 |
| `deltu-aarch64-unknown-linux-gnu` | Linux ARM64 (Raspberry Pi 4/5, etc.) |
| `deltu-aarch64-apple-darwin` | macOS Apple Silicon |
| `SHA256SUMS` | checksums for the above |

```bash
sha256sum -c SHA256SUMS --ignore-missing
chmod +x deltu
./deltu --version
```

Reference numbers (macOS Apple Silicon, release profile, stripped):
binary ≈ 5.2 MB, cold start to `/health` ≈ 0.7 s, ≈ 77k events/s
ingest on loopback (see the Unit 10 baseline in
`context/progress-tracker.md`).

## 2. Run

Minimal (defaults bind `127.0.0.1:8080`):

```bash
./deltu run
```

With a config file (full reference: `deltu check --config <file>`):

```bash
cat > engine.yaml <<'EOF'
http:
  bind: 0.0.0.0:8080
rules:
  - id: door-open-log
    on: !Event
      kind: door
    condition: !Comparison
      field: !EventValue
      op: Eq
      value: !Text
        open
    action: demo-log
actions:
  - id: demo-log
    kind:
      type: log
      level: info
EOF
./deltu run --config engine.yaml
```

### Action kinds

**`log`** — one structured JSON line on stderr:

```yaml
actions:
  - id: demo-log
    kind:
      type: log
      level: info        # debug | info | warn | error
      template: null     # optional, {rule_id} / {payload} placeholders
```

**`webhook`** — delivers the rule snapshot as one JSON document
(`{rule_id, action, payload, ts_ms}`) to an HTTP endpoint. One bounded
synchronous request per firing; **no retries**; failures (connection
refused, timeout, non-2xx) are counted in `/v1/status` →
`actions.failed` and never stop the engine:

```yaml
actions:
  - id: notify-slack
    kind:
      type: webhook
      url: https://example.com/hooks/deltu
      method: post           # post (default) | put | patch
      timeout_ms: 3000       # default 3000, max 30000
      headers:               # optional, max 16 headers
        - name: authorization
          value: Bearer <token>
```

Configuration errors (bad URL, out-of-range timeout) are rejected at
`deltu check` / startup — never mid-run.

Environment overrides: `DELTU_HTTP_BIND`, `DELTU_QUEUE_CAPACITY`,
`DELTU_SNAPSHOT_INTERVAL_SECS`.

Graceful shutdown: SIGINT/SIGTERM drain the queue before exit.

## 3. Docker

```bash
docker build -t deltu:local .
docker run -d -p 8080:8080 --name deltu \
  -v "$PWD/engine.yaml:/config/engine.yaml:ro" \
  deltu:local run --config /config/engine.yaml
curl -s http://127.0.0.1:8080/health
```

The image is distroless, runs as a non-root user, and carries a
`HEALTHCHECK` against `/health`. AI features are excluded from the
default image by design (spec 11 feature-gating).

## 4. Demo: engine + MQTT broker

```bash
docker compose -f docker/compose.yaml up --build
```

Publish a sensor message and watch the rule fire:

```bash
# in another terminal
docker run --rm --network docker_default eclipse-mosquitto:2 \
  mosquitto_pub -h mosquitto -t sensors/esp32-1 -m '{"type":"text","value":"open"}'
```

The engine (subscribed to `sensors/#`) validates the message, feeds the
pipeline/state/rule chain, and the `door-open-log` action writes a
structured log line.

## 5. End-to-end demo (no Docker)

`examples/demo.sh` starts an engine with a demo rule, sends three
simulated sensor events over HTTP, and prints the status snapshot —
the scripted MVP proof (success criterion 25):

```bash
./examples/demo.sh 8211
```

Expected: `accepted [true,true,true]`, `actions_fired 1` (the
`door-open-log` rule fires on the first door event), `state.entries 1`
(both door events share one state key; the numeric reading accumulates
in an open window with no entry yet).

## 6. Upgrades

State is in memory with optional file snapshots (spec 12). To preserve
state across a restart, enable `persistence:` in the config; the new
process restores the snapshot on startup and logs
`restored N state entries`. Format version mismatches fail loudly —
export via your own tooling before major upgrades.

## 7. Security boundary

**The HTTP API has no built-in authentication and must not be exposed
directly to an untrusted network.** The default configuration binds to
loopback (`127.0.0.1`) — keep it that way unless the port is protected:

```text
Internet / Untrusted Network
          ↓
Reverse Proxy / Network Access Control
          ↓
        DELTU
```

Put Deltu behind an authenticating reverse proxy or network access
control when it must accept traffic from other hosts. MQTT brokers and
webhook receivers are part of your trust boundary too — protect
credentials and snapshot files accordingly.

## 8. Scope

No Kubernetes manifests, no Helm charts, no telemetry. GitHub Releases
is the only distribution channel; Docker images are built locally or by
CI (the `ai` variant on tag request only).
