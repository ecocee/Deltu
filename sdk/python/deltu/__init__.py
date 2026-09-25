"""deltu — Python client for the Deltu event engine (public v1 API)."""

from .client import BatchResult, DeltuClient, EventResult, StatusSnapshot
from .errors import (
    DeltuError,
    EngineUnavailableError,
    InvalidEventError,
    NetworkError,
    PayloadTooLargeError,
    QueueFullError,
)
from .events import Event, EventError, boolean, null, numeric, structured, text

__version__ = "0.1.0"

__all__ = [
    "BatchResult",
    "DeltuClient",
    "DeltuError",
    "EngineUnavailableError",
    "Event",
    "EventError",
    "EventResult",
    "InvalidEventError",
    "NetworkError",
    "PayloadTooLargeError",
    "QueueFullError",
    "StatusSnapshot",
    "boolean",
    "null",
    "numeric",
    "structured",
    "text",
]
