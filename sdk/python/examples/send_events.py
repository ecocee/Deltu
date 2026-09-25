"""Runnable example: send one event to a running engine.

Start an engine first:
    deltu run

Then (from sdk/python):
    PYTHONPATH=. python3 examples/send_events.py
"""

import time

from deltu import DeltuClient, Event, text

URL = "http://127.0.0.1:8080"

with DeltuClient(URL) as client:
    print("healthy:", client.health())
    result = client.send_event(
        Event(
            id=f"example-{time.time_ns()}",
            source="example",
            kind="door.state",
            timestamp=int(time.time() * 1000),
            payload=text("open"),
        )
    )
    print("accepted:", result.accepted)
    snapshot = client.status()
    print("state entries:", snapshot.state["entries"])
