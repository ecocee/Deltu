//! Action errors.

use std::fmt;

/// Why an action execution failed. Never panics, never propagates — the
/// dispatcher collects these as outcomes (invariant 8).
#[derive(Debug, Clone, PartialEq)]
pub enum ActionError {
    /// The request named an action id that is not configured.
    UnknownAction(String),
    /// The action executed but its output was rejected (e.g. an executor
    /// refused to write an empty payload).
    ExecutionFailed { action_id: String, reason: String },
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::UnknownAction(id) => {
                write!(f, "unknown action id: {id}")
            }
            ActionError::ExecutionFailed { action_id, reason } => {
                write!(f, "action {action_id} failed: {reason}")
            }
        }
    }
}

impl std::error::Error for ActionError {}

/// Action *definition* errors — configuration-time only, following the
/// project error conventions (spec 01).
#[derive(Debug, Clone, PartialEq)]
pub enum ActionConfigError {
    /// An action id was empty or whitespace-only.
    EmptyActionId(String),
    /// Two action definitions shared the same id.
    DuplicateActionId(String),
    /// A webhook action's configuration was invalid (URL, timeout,
    /// headers). Named at construction time, never at runtime.
    InvalidWebhook {
        /// What was wrong, with the offending value where actionable.
        reason: String,
    },
}

impl fmt::Display for ActionConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionConfigError::EmptyActionId(id) => {
                write!(f, "action id must be a non-empty string, got {id:?}")
            }
            ActionConfigError::DuplicateActionId(id) => {
                write!(f, "duplicate action id: {id}")
            }
            ActionConfigError::InvalidWebhook { reason } => {
                write!(f, "invalid webhook action: {reason}")
            }
        }
    }
}

impl std::error::Error for ActionConfigError {}
