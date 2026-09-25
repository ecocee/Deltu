# Spec 09 — CLI

Status: COMPLETE (implemented & verified 2026-09-25; finalized semantics below) · Depends on: Unit 07 (Runtime & HTTP)

## Goal

Expose the minimal operational CLI (ui-context.md: Docker-Compose-style
familiarity) strictly mirroring runtime capabilities that already exist.
No command is invented before its engine function exists (build plan §22).

## Design

### Commands (v1)

```bash
deltu run [--config <path>]        # foreground service (replaces spec 07 flag surface)
deltu check --config <path>        # validate config, exit 0/1 with actionable errors
deltu status [--url <url>]         # GET /v1/status against a running instance
deltu health  [--url <url>]        # GET /health for scripts and orchestrators
deltu version                      # prints "deltu <semver>"
```

* `up/down/start/stop/logs/config` from ui-context.md are **deferred** until
  a daemon/model exists that makes them meaningful — they are not silently
  dropped; this spec records why they wait (single-process engine; no daemon
  lifecycle yet).
* `status`/`health` are thin HTTP clients over the *documented public API*
  (Unit 07) — the CLI never bypasses the API to read internal state.
* Exit codes: 0 success, 1 validation/connection failure, 2 usage error;
  documented in `--help`.
* Client deps: `clap` (derive) only — no API client framework; JSON
  pretty-printing via serde_json.

### Binary naming (resolves the tracked open question)

The final name decision belongs to this unit's start: the architecture
examples say `plan-c`, the product is Deltu. Recommendation recorded here:
ship as **`deltu`** unless a stronger reason emerges — the crate is already
`deltu`, and one name across crate/binary/docs is worth more than the
example's historical value. Confirmed by whoever starts this unit.

## Implementation

1. `src/bin/deltu.rs` or expanded `main.rs` with clap definitions; subcommand
   dispatch to runtime start (07) or HTTP client calls.
2. Tests: config-check success/failure paths against fixture configs;
   status/health against a stub server; version output; exit codes.
3. Scope guard: no daemonization, no shell completion generation beyond
   clap defaults, no interactive prompts.

## Dependencies

`clap` (derive). Nothing else.

## Finalized at Unit Start (review pass, 2026-09-25)

1. **Binary name resolved: `deltu`** (was the tracked open question; the
   spec's recommendation adopted — one name across crate/binary/docs).
2. **YAML config shape:** `Trigger`/`Condition` stay **externally tagged**
   (serde default; YAML `!Event`/`!Comparison` tags, scalar wrappers like
   `!Numeric`). An internally-tagged attempt (`{type: event, ...}`) was
   made and reverted: on this mutually-recursive enum tree it produces a
   rustc E0275 overflow in the serde derive that balloons test
   compilation (the "cargo test takes long" report this unit). Rule
   enums use PascalCase variants in config (`EventValue`, `Gt`) — no
   `rename_all` — while `ActionKind` remains internally tagged
   (`type: log`) because it is not recursive.
3. **`status`/`health` HTTP client:** zero-dependency hand-rolled
   HTTP/1.1 GET over `TcpStream` (one blocking call per invocation) — a
   client crate for two GETs violates the dependency rule. `clap` 4.6 is
   the only new dependency.
4. **Exit codes** implemented as specified: 0 success, 1
   validation/connection failure, 2 usage error (unknown command/flag).

## Verify When Done

* [ ] Full cargo suite clean; prior tests green.
* [ ] Every command documented in `--help` behaves as specified; exit codes
      tested.
* [ ] Tracker updated (results + notes, including the binary-name decision).
