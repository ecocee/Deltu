//! Deltu's in-memory current-state store: the keyed, bounded, expiring
//! snapshot of "what is true now" (decision 002 — memory-first; invariant 10
//! — runtime state is separate from historical persistence).

pub mod store;

pub use store::{StateEntry, StateKey, StateStore, StateValue};

/// Configuration for [`StateStore`](store::StateStore). Defaults are
/// conservative for edge-class devices and are a documented starting point,
/// not performance claims.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateConfig {
    /// Maximum retained entries regardless of input volume. Default 10_000.
    pub max_entries: usize,
    /// Expiration age in milliseconds; `None` disables expiration.
    /// Default `Some(300_000)` — 5 minutes.
    pub expire_after_ms: Option<i64>,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            expire_after_ms: Some(300_000),
        }
    }
}

/// Store errors, following the project error conventions (spec 01):
/// actionable Display, `std::error::Error`.
#[derive(Debug, Clone, PartialEq)]
pub enum StateError {
    /// `max_entries` must be greater than zero.
    InvalidCapacity(usize),
    /// `expire_after_ms`, when set, must be greater than zero.
    InvalidExpireAfter(i64),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::InvalidCapacity(value) => {
                write!(f, "max_entries must be greater than 0, got {value}")
            }
            StateError::InvalidExpireAfter(value) => {
                write!(
                    f,
                    "expire_after_ms must be greater than 0 when set, got {value}"
                )
            }
        }
    }
}

impl std::error::Error for StateError {}

/// Drop/expiration counters, incremented only on real events — drops must
/// be observable from day one (same pattern as `PipelineCounters`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StateCounters {
    /// Entries removed by [`StateStore::expire`].
    pub expired: u64,
    /// Entries evicted by the capacity cap (stalest first).
    pub evicted: u64,
}
