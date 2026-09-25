//! Rule engine configuration errors and evaluation failures.
//!
//! Evaluation is total by design: comparisons against missing state or
//! type-mismatched values are `false`, never errors — so there is no
//! runtime `RuleError`. Only configuration can be invalid.

use std::fmt;

/// Errors produced when constructing a [`RuleEngine`](crate::RuleEngine)
/// from rule definitions. Variants map one-to-one to the validation rules
/// in `context/specs/05-rules.md`.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleConfigError {
    /// A rule id was empty or whitespace-only.
    EmptyRuleId,
    /// Two rules shared the same id.
    DuplicateRuleId(String),
    /// An action id was empty or whitespace-only.
    EmptyActionId,
    /// `All` or `Any` was constructed with an empty condition list —
    /// rejected so a malformed config cannot silently always-fire.
    EmptyConditionList(String),
    /// A suppression window must be greater than zero.
    InvalidSuppressionWindow { rule_id: String, window_ms: i64 },
    /// A comparison literal must be scalar (Numeric, Text, or Boolean);
    /// structured values are not comparable.
    NonScalarLiteral { rule_id: String },
}

impl fmt::Display for RuleConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleConfigError::EmptyRuleId => {
                write!(f, "rule id must be a non-empty string")
            }
            RuleConfigError::DuplicateRuleId(id) => {
                write!(f, "duplicate rule id: {id}")
            }
            RuleConfigError::EmptyActionId => {
                write!(f, "rule action id must be a non-empty string")
            }
            RuleConfigError::EmptyConditionList(rule_id) => {
                write!(
                    f,
                    "rule {rule_id}: All/Any requires at least one condition; \
                     an empty list would always fire"
                )
            }
            RuleConfigError::InvalidSuppressionWindow { rule_id, window_ms } => {
                write!(
                    f,
                    "rule {rule_id}: suppression window_ms must be greater than 0, \
                     got {window_ms}"
                )
            }
            RuleConfigError::NonScalarLiteral { rule_id } => {
                write!(
                    f,
                    "rule {rule_id}: comparison values must be Numeric, Text, \
                     or Boolean — structured values are not comparable"
                )
            }
        }
    }
}

impl std::error::Error for RuleConfigError {}
