//! Rule definitions — config-shaped, serde-derived, closed operator set.
//!
//! No scripting language and no string evaluation (research/rules.md);
//! conditions are typed trees over a closed operator set.

use serde::{Deserialize, Serialize};

/// What causes a rule to be considered for firing.
///
/// Externally tagged (serde default). YAML configs use `!Event` tags; the
/// JSON API is unaffected. (Internally-tagged representation on this
/// mutually-recursive enum tree triggers a rustc E0275 overflow in the
/// derive — see the git history of this file for the attempt.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Trigger {
    /// Fires on inputs of one kind family (e.g. `"temperature"` matches
    /// `temperature.reading`) — from a raw event or a window summary.
    Event {
        /// The kind (or dotted kind family) to match.
        kind: String,
    },
    /// Considered on every input; fires when its state key currently
    /// exists (absence-based rules such as "went offline" need periodic
    /// evaluation and belong to the runtime unit).
    State {
        /// State source to watch.
        source: String,
        /// State kind to watch.
        kind: String,
    },
}

/// Which value a condition reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Field {
    /// The triggering input's payload value (event payload or summary mean).
    EventValue,
    /// A current state entry's value; missing state compares as `false`.
    State {
        /// State source to read.
        source: String,
        /// State kind to read.
        kind: String,
    },
    /// A statistic of the triggering window summary; `false` when the
    /// triggering input is a raw event.
    Aggregation {
        /// Which statistic of the summary to read.
        stat: Stat,
    },
}

/// A statistic of a closed aggregation window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stat {
    /// Arithmetic mean of the window.
    Mean,
    /// Minimum of the window.
    Min,
    /// Maximum of the window.
    Max,
    /// Sum of the window.
    Sum,
    /// Number of events in the window.
    Count,
    /// Last value accumulated in the window.
    Last,
}

/// A scalar comparable literal. Structured values are rejected at
/// configuration time (non-scalar comparisons are not well-defined).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    /// Numeric comparison value.
    Numeric(f64),
    /// Text comparison value.
    Text(String),
    /// Boolean comparison value.
    Boolean(bool),
}

/// The closed operator set. Ordering operators apply to numerics only;
/// text and booleans support equality (`research/rules.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operator {
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
}

/// Typed condition tree. `All`/`Any` require at least one child — enforced
/// at configuration time so a malformed rule cannot silently always-fire.
/// Externally tagged (serde default); see [`Trigger`] for why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// True when every child is true.
    All(Vec<Condition>),
    /// True when any child is true.
    Any(Vec<Condition>),
    /// True when the child is false.
    Not(Box<Condition>),
    /// A typed comparison against a scalar literal.
    Comparison {
        /// Which value is read.
        field: Field,
        /// Which operator applies.
        op: Operator,
        /// The scalar literal to compare against.
        value: Literal,
    },
    /// True when the referenced value exists (event payload always does;
    /// a state key must currently be present; `Aggregation` exists only on
    /// summary inputs).
    Exists {
        /// Which value is probed.
        field: Field,
    },
}

/// Trigger-suppression policy: fire at most once per event-time window
/// (same tumbling logic as the aggregator — clock-free and deterministic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OncePerWindow {
    /// Tumbling window length in event-time milliseconds.
    pub window_ms: i64,
}

/// A rule definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// Unique, non-empty identifier.
    pub id: String,
    /// Optional human description.
    pub description: Option<String>,
    /// What causes this rule to be considered.
    pub on: Trigger,
    /// The condition that must hold for the rule to fire.
    pub condition: Condition,
    /// Action id from configuration (Unit 06 resolves it).
    pub action: String,
    /// Optional once-per-window suppression.
    pub suppression: Option<OncePerWindow>,
}
