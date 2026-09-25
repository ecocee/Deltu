# Developer Context

Deltu is a **developer-first engine**. Developers interact with it through:

```text
CLI
API
SDK
Configuration
Logs
Metrics
```

Deltu does **not** require a graphical UI. See `ui-context.md` for the
interface philosophy.

## CLI

The CLI is the primary operational interface. Its operational style should be
lightweight and familiar, similar to Docker Compose:

```bash
plan-c up
plan-c down
plan-c start
plan-c stop
plan-c status
plan-c logs
plan-c config
```

CLI commands are implemented only when the corresponding runtime functionality
exists. Do not build a large CLI before the engine exists.

> Open question: the final binary name (`plan-c` in the examples vs. the
> project name Deltu) is tracked in `progress-tracker.md` and must be
> confirmed before the CLI unit.

## API

The HTTP/JSON API is the universal, language-agnostic integration boundary.
It exposes event ingestion, health, runtime status, and the interfaces listed
in `architecture.md`. Public API behavior must be documented and tested before
release; handlers validate input before calling core interfaces.

## SDK

Official SDKs provide thin integration layers over the public API:

* Python SDK — for Python applications.
* TypeScript/JavaScript SDK — for Node.js, TypeScript, and web applications.

SDKs communicate with the Deltu engine through documented APIs; they never
reimplement processing, state, or rules.

## Configuration

Runtime configuration uses YAML, JSON, or environment variables (see the
stack table in `architecture.md`). Configuration is loaded at startup and
explicitly validated; no silent defaults that mask misconfiguration.

## Logs

Structured logging for runtime diagnostics: events, errors, adapter
failures, action results, and lifecycle information. Logs must make failures
explicit — the runtime must never appear successful when processing failed.

## Metrics

Lightweight internal metrics covering:

* Events per second, latency (average, P95, P99)
* CPU and memory usage
* Queue depth and backpressure
* Errors, dropped events, failed actions
* AI calls, tokens, latency, and calls avoided (when AI is configured)

Metrics are measured, not claimed. Performance statements must be backed by
benchmark results (see `research/performance.md`).
