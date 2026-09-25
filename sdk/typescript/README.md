# @deltu/client

TypeScript client for the Deltu event engine's public HTTP API (v1).
Zero runtime dependencies (built-in `fetch`); no processing logic (the
engine is the only validator of record). Strict-mode TypeScript, no `any`.

## License

Apache-2.0 — Copyright 2026 ECOCEE. See the repository root
[`LICENSE`](../../LICENSE) and [`NOTICE`](../../NOTICE).

## Install

```bash
npm install ./sdk/typescript
```

## Example

Start an engine:

```bash
deltu run   # binds 127.0.0.1:8080
```

Send events:

```js
import { DeltuClient, Event, text } from "@deltu/client";

const client = new DeltuClient("http://127.0.0.1:8080");

console.log(await client.health()); // true

const result = await client.sendEvent(new Event({
  id: "demo-1",
  source: "sensor-1",
  kind: "door.state",
  timestamp: Date.now(),
  payload: text("open"),
}));
console.log(result.accepted); // true

const snapshot = await client.status();
console.log(snapshot.state.entries);
```

## Errors

Every documented HTTP failure is a distinct class: `InvalidEventError`
(400), `PayloadTooLargeError` (413), `QueueFullError` (429),
`EngineUnavailableError` (503), plus `NetworkError` when the engine is
unreachable. Retries are opt-in (`retries`, exponential `backoffMs`) and
apply to 429/503 only — never to 400/413.

## Tests

```bash
cd sdk/typescript && npm test
```

The suite runs against a stub server; an end-to-end test runs against a
live engine on `127.0.0.1:8210` when one is reachable.
