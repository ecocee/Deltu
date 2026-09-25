# deltu — Python client

Thin client for the Deltu event engine's public HTTP API (v1). Zero
runtime dependencies beyond `httpx`; no processing logic (the engine is
the only validator of record).

## License

Apache-2.0 — Copyright 2026 ECOCEE. See the repository root
[`LICENSE`](../../LICENSE) and [`NOTICE`](../../NOTICE).

## Install

```bash
pip install ./sdk/python
```

## Example

Start an engine:

```bash
deltu run   # binds 127.0.0.1:8080
```

Send events:

```python
import time
from deltu import DeltuClient, Event, numeric, text

with DeltuClient("http://127.0.0.1:8080") as client:
    assert client.health()

    result = client.send_event(
        Event(
            id="demo-1",
            source="sensor-1",
            kind="door.state",
            timestamp=int(time.time() * 1000),
            payload=text("open"),
        )
    )
    print(result.accepted)  # True

    snapshot = client.status()
    print(snapshot.state["entries"])
```

## Errors

Every documented HTTP failure is a distinct type: `InvalidEventError`
(400), `PayloadTooLargeError` (413), `QueueFullError` (429),
`EngineUnavailableError` (503), plus `NetworkError` when the engine is
unreachable. Retries are opt-in (`retries=`, backoff with jitter-free
exponential delay) and apply to 429/503 only — never to 400/413.

## Tests

```bash
python3 -m pytest sdk/python/tests -q
```
