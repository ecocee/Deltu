"""Deltu Python client — a thin HTTP wrapper around the public v1 API.

Zero runtime dependencies beyond `httpx`; no processing logic lives here
(invariant 15: SDKs communicate, never reimplement).
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Any, Dict, List, Optional

import httpx

from .errors import (
    DeltuError,
    EngineUnavailableError,
    NetworkError,
    QueueFullError,
    error_for_response,
)
from .events import Event

__all__ = ["DeltuClient", "BatchResult", "EventResult", "StatusSnapshot", "DeltuError"]

_DEFAULT_RETRIES = 0
_DEFAULT_BACKOFF_SECS = 0.2
_RETRYABLE = (429, 503)


@dataclass(frozen=True)
class BatchResult:
    """Result of `POST /v1/events`."""

    accepted: List[bool]
    actions_fired: int


@dataclass(frozen=True)
class EventResult:
    """Result of a single-event send."""

    accepted: bool
    actions_fired: int


class StatusSnapshot:
    """Mirrors the engine's `/v1/status` export field-for-field (spec 10)."""

    def __init__(self, data: Dict[str, Any]) -> None:
        self.raw = data
        self.version: str = str(data.get("version", ""))
        self.uptime_seconds: int = int(data.get("uptime_seconds", 0))
        self.queue_depth: int = int(data.get("queue_depth", 0))
        pipeline = data.get("pipeline") or {}
        self.pipeline = {
            "filtered_out": int(pipeline.get("filtered_out", 0)),
            "duplicates": int(pipeline.get("duplicates", 0)),
            "late_dropped": int(pipeline.get("late_dropped", 0)),
            "windows_evicted": int(pipeline.get("windows_evicted", 0)),
            "change_states_evicted": int(pipeline.get("change_states_evicted", 0)),
            "changes_suppressed": int(pipeline.get("changes_suppressed", 0)),
        }
        state = data.get("state") or {}
        self.state = {
            "entries": int(state.get("entries", 0)),
            "expired": int(state.get("expired", 0)),
            "evicted": int(state.get("evicted", 0)),
        }
        self.metrics: Dict[str, Any] = dict(data.get("metrics") or {})

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"StatusSnapshot(version={self.version!r}, queue_depth={self.queue_depth})"


class DeltuClient:
    """Client for a running Deltu engine (`deltu run`)."""

    def __init__(
        self,
        url: str = "http://127.0.0.1:8080",
        api_key: Optional[str] = None,
        timeout: float = 10.0,
        retries: int = _DEFAULT_RETRIES,
        backoff_secs: float = _DEFAULT_BACKOFF_SECS,
    ) -> None:
        self._url = url.rstrip("/")
        self._retries = retries
        self._backoff = backoff_secs
        headers = {"accept": "application/json"}
        if api_key is not None:
            headers["authorization"] = f"Bearer {api_key}"
        self._http = httpx.Client(timeout=timeout, headers=headers)

    def close(self) -> None:
        self._http.close()

    def __enter__(self) -> "DeltuClient":
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()

    # -- public surface -------------------------------------------------

    def send_events(self, events: List[Event]) -> BatchResult:
        """`POST /v1/events` with a batch (1..max_batch events)."""
        if not events:
            raise ValueError("send_events requires at least one event")
        body = {"events": [event.to_json() for event in events]}
        data = self._post("/v1/events", body)
        payload = data.get("data") or {}
        return BatchResult(
            accepted=[bool(v) for v in payload.get("accepted", [])],
            actions_fired=int(payload.get("actions_fired", 0)),
        )

    def send_event(self, event: Event) -> EventResult:
        """Single-event convenience over the same endpoint."""
        batch = self.send_events([event])
        return EventResult(
            accepted=batch.accepted[0] if batch.accepted else False,
            actions_fired=batch.actions_fired,
        )

    def health(self) -> bool:
        """`GET /health` — liveness probe."""
        try:
            response = self._http.get(f"{self._url}/health")
        except httpx.HTTPError as error:
            raise NetworkError(f"engine unreachable at {self._url}: {error}") from error
        if response.status_code >= 400:
            raise error_for_response(response.status_code, _safe_json(response))
        return response.status_code < 400

    def status(self) -> StatusSnapshot:
        """`GET /v1/status` — counters snapshot."""
        data = self._get("/v1/status")
        return StatusSnapshot(data.get("data") or {})

    # -- transport -------------------------------------------------------

    def _get(self, path: str) -> Dict[str, Any]:
        try:
            response = self._http.get(f"{self._url}{path}")
        except httpx.HTTPError as error:
            raise NetworkError(f"engine unreachable at {self._url}: {error}") from error
        return self._checked(response)

    def _post(self, path: str, body: Dict[str, Any]) -> Dict[str, Any]:
        attempt = 0
        while True:
            try:
                response = self._http.post(f"{self._url}{path}", json=body)
            except httpx.HTTPError as error:
                raise NetworkError(f"engine unreachable at {self._url}: {error}") from error
            if response.status_code in _RETRYABLE and attempt < self._retries:
                attempt += 1
                time.sleep(self._backoff * (2 ** (attempt - 1)))
                continue
            return self._checked(response)

    def _checked(self, response: httpx.Response) -> Dict[str, Any]:
        if response.status_code >= 400:
            raise error_for_response(response.status_code, _safe_json(response))
        body = _safe_json(response)
        if not isinstance(body, dict) or body.get("success") is not True:
            raise DeltuError(f"unexpected response envelope: {body!r}")
        return body


def _safe_json(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError:
        return {"success": False, "error": {"code": "unknown", "message": response.text[:200]}}
