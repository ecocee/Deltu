"""Tests for the deltu Python client: local validation, typed errors,
envelope handling, retries — plus a live end-to-end test against a real
`deltu run` (skipped when no engine is reachable)."""

from __future__ import annotations

import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from deltu import (
    DeltuClient,
    DeltuError,
    EngineUnavailableError,
    Event,
    EventError,
    InvalidEventError,
    NetworkError,
    PayloadTooLargeError,
    QueueFullError,
    numeric,
    structured,
    text,
)

# ---------------------------------------------------------------------------
# Local event validation (mirrors the engine's rules)


def test_event_rejects_empty_fields():
    with pytest.raises(EventError):
        Event(id="  ", source="s", kind="k", timestamp=1, payload=text("v"))
    with pytest.raises(EventError):
        Event(id="e", source="", kind="k", timestamp=1, payload=text("v"))
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="", timestamp=1, payload=text("v"))


def test_event_rejects_bad_timestamp():
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=0, payload=text("v"))
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=-5, payload=text("v"))


def test_event_rejects_bad_payloads():
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=1, payload=text("   "))
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=1, payload=numeric(float("inf")))
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=1, payload={"type": "numeric"})
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=1, payload={"type": "wat"})


def test_structured_depth_is_bounded():
    deep: dict = {}
    node = deep
    for _ in range(20):
        node["child"] = {}
        node = node["child"]
    with pytest.raises(EventError):
        Event(id="e", source="s", kind="k", timestamp=1, payload=structured(deep))


def test_to_json_normalizes_and_tags():
    event = Event(id=" e1 ", source="s", kind="Door.State", timestamp=10, payload=text("open"))
    wire = event.to_json()
    assert wire["kind"] == "door.state"
    assert wire["payload"] == {"type": "text", "value": "open"}


# ---------------------------------------------------------------------------
# A tiny stub engine standing in for `deltu run`


class _StubHandler(BaseHTTPRequestHandler):
    def log_message(self, *_args):  # silence request logging
        pass

    def _json(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/health":
            self._json(200, {"status": "ok"})
        elif self.path == "/v1/status":
            self._json(200, {"success": True, "data": {"version": "0.1.0", "queue_depth": 0,
                                                        "state": {"entries": 2}}})
        else:
            self._json(404, {"success": False, "error": {"code": "unknown", "message": "nope"}})

    def do_POST(self):
        length = int(self.headers.get("content-length", 0))
        raw = self.rfile.read(length)
        if self.path != "/v1/events":
            self._json(404, {"success": False, "error": {"code": "unknown", "message": "nope"}})
            return
        body = json.loads(raw)
        if body["events"][0]["id"] == "bad":
            self._json(400, {"success": False, "error": {
                "code": "invalid_event", "message": "events[0] failed validation: bad kind"}})
        elif body["events"][0]["id"] == "big":
            self._json(413, {"success": False, "error": {
                "code": "payload_too_large", "message": "too large"}})
        elif body["events"][0]["id"] == "full":
            self._json(429, {"success": False, "error": {
                "code": "queue_full", "message": "queue is full"}})
        elif body["events"][0]["id"] == "down":
            self._json(503, {"success": False, "error": {
                "code": "shutting_down", "message": "shutting down"}})
        else:
            self._json(202, {"success": True,
                             "data": {"accepted": [True] * len(body["events"]),
                                      "actions_fired": 0}})


@pytest.fixture()
def stub_url():
    server = ThreadingHTTPServer(("127.0.0.1", 0), _StubHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_address[1]}"
    server.shutdown()
    server.server_close()


def _event(event_id="e1"):
    return Event(id=event_id, source="s", kind="k", timestamp=1, payload=text("v"))


def test_health_and_status(stub_url):
    with DeltuClient(stub_url) as client:
        assert client.health() is True
        snapshot = client.status()
        assert snapshot.version == "0.1.0"
        assert snapshot.state["entries"] == 2


def test_send_events_round_trip(stub_url):
    with DeltuClient(stub_url) as client:
        batch = client.send_events([_event(), _event("e2")])
        assert batch.accepted == [True, True]
        single = client.send_event(_event())
        assert single.accepted is True


def test_typed_errors_map_status_codes(stub_url):
    with DeltuClient(stub_url) as client:
        with pytest.raises(InvalidEventError) as info:
            client.send_event(_event("bad"))
        assert info.value.code == "invalid_event"
        assert "events[0]" in str(info.value)
        with pytest.raises(PayloadTooLargeError):
            client.send_event(_event("big"))
        with pytest.raises(QueueFullError):
            client.send_event(_event("full"))
        with pytest.raises(EngineUnavailableError):
            client.send_event(_event("down"))


def test_bad_envelope_is_an_error(stub_url):
    with DeltuClient(stub_url) as client:
        with pytest.raises(DeltuError):
            # 404 from the stub has no `data` field → envelope violation.
            client._get("/nope")  # noqa: SLF001 - testing the private path


def test_unreachable_engine_raises_network_error():
    client = DeltuClient("http://127.0.0.1:9", timeout=0.2)
    try:
        with pytest.raises(NetworkError):
            client.health()
    finally:
        client.close()


def test_retries_only_retryable_statuses(stub_url):
    attempts = {"n": 0}

    class Flaky(_StubHandler):
        def do_POST(self):
            attempts["n"] += 1
            if attempts["n"] < 3:
                self._json(429, {"success": False, "error": {
                    "code": "queue_full", "message": "queue is full"}})
                return
            super().do_POST()

    import socket
    server = ThreadingHTTPServer(("127.0.0.1", 0), Flaky)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with DeltuClient(
            f"http://127.0.0.1:{server.server_address[1]}", retries=3, backoff_secs=0.01
        ) as client:
            result = client.send_event(_event())
            assert result.accepted is True
        assert attempts["n"] == 3  # two 429s retried, third attempt succeeded
    finally:
        server.shutdown()
        server.server_close()
    _ = socket  # imported here only to keep the test self-contained
    _ = time  # and silence unused-import linters in constrained environments


# ---------------------------------------------------------------------------
# Live end-to-end against a real engine (skipped when none is running)

LIVE_URL = "http://127.0.0.1:8210"


def _engine_reachable() -> bool:
    import http.client

    try:
        connection = http.client.HTTPConnection("127.0.0.1", 8210, timeout=0.5)
        connection.request("GET", "/health")
        response = connection.getresponse()
        connection.close()
        return response.status == 200
    except OSError:
        return False


@pytest.mark.skipif(not _engine_reachable(), reason="no live engine on 127.0.0.1:8210")
def test_live_engine_end_to_end():
    with DeltuClient(LIVE_URL) as client:
        assert client.health() is True
        result = client.send_event(Event(
            id=f"py-live-{time.time_ns()}",
            source="sdk-python",
            kind="door.state",
            timestamp=int(time.time() * 1000),
            payload=text("open"),
        ))
        assert result.accepted is True
        assert "entries" in client.status().state
