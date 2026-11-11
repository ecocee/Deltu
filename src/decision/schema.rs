//! Typed Decision Schemas for the Deltu System 1 Decision Engine.
//!
//! Inspired by the non-autoregressive decision paradigm:
//! - Choice: pick the best candidate from an option set with calibrated confidence.
//! - Score: continuous calibrated score in [0.0, 1.0].
//! - YesNo: binary calibrated decision with probability.

use serde::{Deserialize, Serialize};

/// The schema specifying what type of decision to evaluate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionSchema {
    /// Select one candidate from a set of options (e.g., intent routing, categorization).
    Choice {
        /// Candidate options to evaluate against the input.
        options: Vec<String>,
        /// Temperature scaling parameter for confidence calibration (default: 10.0).
        #[serde(default = "default_temperature")]
        temperature: f32,
    },
    /// Rate the input on a continuous scale [min, max] (e.g. priority, urgency, sentiment).
    Score {
        /// The criteria or target concept to measure against (e.g. "urgent emergency", "positive sentiment").
        criteria: String,
        /// Minimum scale bound (default: 0.0).
        #[serde(default = "default_min")]
        min: f64,
        /// Maximum scale bound (default: 1.0).
        #[serde(default = "default_max")]
        max: f64,
    },
    /// Binary yes/no decision with calibrated probability (e.g. guardrails, approval gates).
    YesNo {
        /// Condition statement to verify (e.g. "request contains billing inquiry", "is hostile").
        condition: String,
        /// Probability threshold required for a 'true' verdict (default: 0.5).
        #[serde(default = "default_threshold")]
        threshold: f32,
    },
}

fn default_temperature() -> f32 {
    10.0
}

fn default_min() -> f64 {
    0.0
}

fn default_max() -> f64 {
    1.0
}

fn default_threshold() -> f32 {
    0.5
}

/// Request to evaluate a decision over an input text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequest {
    /// Optional identifier for tracing.
    pub id: Option<String>,
    /// The input text / document / query to evaluate.
    pub input: String,
    /// The decision schema to execute.
    pub schema: DecisionSchema,
}

/// Individual candidate score in a Choice decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOptionScore {
    /// Option label.
    pub option: String,
    /// Calibrated probability [0.0, 1.0] sum to 1.0 across options.
    pub probability: f32,
    /// Raw geometric cosine similarity [-1.0, 1.0].
    pub raw_similarity: f32,
}

/// Outcome of a decision evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionOutcome {
    /// Outcome of a Choice decision.
    Choice {
        /// The highest-probability selected option.
        selected: String,
        /// Calibrated confidence of the top selection [0.0, 1.0].
        confidence: f32,
        /// All ranked candidate probabilities.
        rankings: Vec<ChoiceOptionScore>,
    },
    /// Outcome of a Score decision.
    Score {
        /// Criteria evaluated.
        criteria: String,
        /// Value projected onto [min, max].
        score: f64,
        /// Normalized score in [0.0, 1.0].
        normalized: f64,
        /// Raw similarity before projection.
        raw_similarity: f32,
    },
    /// Outcome of a Yes/No decision.
    YesNo {
        /// Condition evaluated.
        condition: String,
        /// Binary verdict: true if probability >= threshold.
        result: bool,
        /// Calibrated probability of the positive verdict [0.0, 1.0].
        probability: f32,
        /// The threshold applied.
        threshold: f32,
    },
}

/// Complete response wrapping the decision outcome with execution metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResponse {
    /// Request ID if provided, otherwise auto-generated.
    pub id: String,
    /// The typed decision outcome.
    pub outcome: DecisionOutcome,
    /// Wall-clock evaluation latency in microseconds.
    pub latency_us: u64,
}

