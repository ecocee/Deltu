"""Event model mirroring the engine's validation (spec 02) — a cheap
pre-flight, not a second validator of record: the server remains
authoritative and the SDK surfaces its errors verbatim."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Dict, Optional

MAX_STRUCTURED_DEPTH = 16


class EventError(ValueError):
    """Local validation failure; message text matches the engine's."""


@dataclass(frozen=True)
class Event:
    """One Deltu event. Mirrors `spec 02` field-for-field."""

    id: str
    source: str
    kind: str
    timestamp: int
    payload: Dict[str, Any]

    def __post_init__(self) -> None:
        self.validate()

    def validate(self) -> "Event":
        """Same rules as the engine; raises :class:`EventError`."""
        if not self.id.strip():
            raise EventError("id must not be empty")
        if not self.source.strip():
            raise EventError("source must not be empty")
        if not self.kind.strip():
            raise EventError("kind must not be empty")
        if self.timestamp <= 0:
            raise EventError(f"invalid timestamp: {self.timestamp}")
        _validate_payload(self.payload)
        return self

    def to_json(self) -> Dict[str, Any]:
        """Wire shape: payload is inline with a `type` tag."""
        payload = dict(self.payload)
        payload.setdefault("type", _infer_payload_type(payload))
        return {
            "id": self.id,
            "source": self.source,
            "kind": self.kind.lower(),
            "timestamp": self.timestamp,
            "payload": payload,
        }


def _infer_payload_type(payload: Dict[str, Any]) -> str:
    explicit = payload.get("type")
    if isinstance(explicit, str):
        return explicit
    keys = set(payload.keys())
    if keys == {"type", "value"}:
        value = payload["value"]
        if isinstance(value, bool):
            return "boolean"
        if isinstance(value, (int, float)):
            return "numeric"
        if isinstance(value, str):
            return "text"
        if isinstance(value, dict):
            return "structured"
        if value is None:
            return "null"
    raise EventError("payload needs an explicit `type` tag")


def text(value: str) -> Dict[str, Any]:
    """Text payload."""
    return {"type": "text", "value": value}


def numeric(value: float) -> Dict[str, Any]:
    """Numeric payload."""
    return {"type": "numeric", "value": value}


def boolean(value: bool) -> Dict[str, Any]:
    """Boolean payload."""
    return {"type": "boolean", "value": value}


def structured(value: Dict[str, Any]) -> Dict[str, Any]:
    """Structured (JSON object) payload."""
    return {"type": "structured", "value": value}


def null() -> Dict[str, Any]:
    """Null payload."""
    return {"type": "null"}


def _validate_payload(payload: Any) -> None:
    if not isinstance(payload, dict):
        raise EventError("payload must be an object with a `type` tag")
    ptype = payload.get("type")
    if not isinstance(ptype, str):
        raise EventError("payload must carry a string `type` tag")
    value = payload.get("value")
    if ptype == "numeric":
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise EventError("numeric payload requires a number value")
        if isinstance(value, float) and not math.isfinite(value):
            raise EventError("numeric payload must be finite")
    elif ptype == "text":
        if not isinstance(value, str) or not value.strip():
            raise EventError("text payload must not be empty or whitespace-only")
    elif ptype == "boolean":
        if not isinstance(value, bool):
            raise EventError("boolean payload requires a boolean value")
    elif ptype == "structured":
        if not isinstance(value, dict):
            raise EventError("structured payload requires a JSON object value")
        _check_depth(value, 0)
    elif ptype == "null":
        if "value" in payload and payload["value"] is not None:
            raise EventError("null payload requires value null or no value key")
    else:
        raise EventError(f"unknown payload type: {ptype!r}")


def _check_depth(value: Any, depth: int) -> None:
    if depth > MAX_STRUCTURED_DEPTH:
        raise EventError("structured payload exceeds maximum depth of 16")
    if isinstance(value, dict):
        for child in value.values():
            _check_depth(child, depth + 1)
    elif isinstance(value, list):
        for child in value:
            _check_depth(child, depth + 1)
