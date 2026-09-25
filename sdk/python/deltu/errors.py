"""Typed errors mirroring the engine's documented HTTP envelope (spec 07):
every status code is a distinct exception type."""

from __future__ import annotations

from typing import Any, Dict, Optional


class DeltuError(Exception):
    """Base for all client errors."""


class InvalidEventError(DeltuError):
    """HTTP 400 — `error.code == "invalid_event"`."""

    def __init__(self, message: str, code: str = "invalid_event",
                 body: Optional[Dict[str, Any]] = None) -> None:
        super().__init__(message)
        self.code = code
        self.body = body


class PayloadTooLargeError(DeltuError):
    """HTTP 413 — `error.code == "payload_too_large"`."""


class QueueFullError(DeltuError):
    """HTTP 429 — `error.code == "queue_full"` (retryable)."""


class EngineUnavailableError(DeltuError):
    """HTTP 503 — `error.code == "shutting_down"` (retryable)."""


class NetworkError(DeltuError):
    """The engine could not be reached at all."""


_STATUS_TO_ERROR = {
    400: InvalidEventError,
    413: PayloadTooLargeError,
    429: QueueFullError,
    503: EngineUnavailableError,
}


def error_for_response(status: int, body: Any) -> DeltuError:
    """Maps a non-2xx response to the matching typed error."""
    message = "the engine rejected the request"
    code = "unknown"
    if isinstance(body, dict) and isinstance(body.get("error"), dict):
        error = body["error"]
        message = str(error.get("message", message))
        code = str(error.get("code", code))
    cls = _STATUS_TO_ERROR.get(status, DeltuError)
    if cls is InvalidEventError:
        return InvalidEventError(message, code=code, body=body)
    instance = cls(message)
    instance.code = code  # type: ignore[attr-defined]
    instance.status = status  # type: ignore[attr-defined]
    return instance
