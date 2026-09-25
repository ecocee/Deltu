//! Event validation errors.

use std::fmt;

/// Errors produced when constructing or validating an [`Event`](crate::Event).
///
/// Variants map one-to-one to the validation rules in
/// `context/specs/02-event-model.md` so callers can react to specific
/// failures instead of parsing messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    /// Event id was empty or whitespace-only.
    EmptyId,
    /// Event source was empty or whitespace-only.
    EmptySource,
    /// Event kind was empty or whitespace-only.
    EmptyKind,
    /// Timestamp was not a positive Unix epoch millisecond value.
    InvalidTimestamp(i64),
    /// Payload was rejected (non-finite number or empty text).
    InvalidPayload(&'static str),
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::EmptyId => {
                write!(f, "event id must be a non-empty string")
            }
            EventError::EmptySource => {
                write!(f, "event source must be a non-empty string")
            }
            EventError::EmptyKind => {
                write!(f, "event kind must be a non-empty string")
            }
            EventError::InvalidTimestamp(ts) => {
                write!(
                    f,
                    "event timestamp must be a positive Unix epoch millisecond value, got {ts}"
                )
            }
            EventError::InvalidPayload(reason) => {
                write!(f, "invalid event payload: {reason}")
            }
        }
    }
}

impl std::error::Error for EventError {}
