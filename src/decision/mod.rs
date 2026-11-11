//! Deltu Decision Engine: sub-millisecond, non-autoregressive "System 1" decisions.
//!
//! Exposes typed choice selection, continuous scoring, and binary gates over text.

pub mod mcp;
pub mod schema;
pub mod vector;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub use mcp::{handle_mcp_message, run_mcp_server};
pub use schema::{
    ChoiceOptionScore, DecisionOutcome, DecisionRequest, DecisionResponse, DecisionSchema,
};
pub use vector::{TextVector, evaluate_decision};

/// Thread-safe counters for the decision engine.
#[derive(Debug, Default)]
pub struct DecisionCounters {
    /// Total decisions evaluated.
    pub decisions_total: AtomicU64,
    /// Choice decisions evaluated.
    pub choices_evaluated: AtomicU64,
    /// Score decisions evaluated.
    pub scores_evaluated: AtomicU64,
    /// Yes/No decisions evaluated.
    pub yesno_evaluated: AtomicU64,
    /// Total latency accumulated in microseconds.
    pub total_latency_us: AtomicU64,
}

/// The main Decision Engine instance.
#[derive(Debug, Default)]
pub struct DecisionEngine {
    counters: DecisionCounters,
}

impl DecisionEngine {
    /// Creates a fresh decision engine.
    pub fn new() -> Self {
        Self {
            counters: DecisionCounters::default(),
        }
    }

    /// Evaluates a typed decision request.
    pub fn evaluate(&self, req: DecisionRequest) -> Result<DecisionResponse, String> {
        let started = Instant::now();
        let outcome = evaluate_decision(&req.input, &req.schema)?;
        let elapsed_us = started.elapsed().as_micros() as u64;

        self.counters
            .decisions_total
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .total_latency_us
            .fetch_add(elapsed_us, Ordering::Relaxed);

        match &outcome {
            DecisionOutcome::Choice { .. } => {
                self.counters
                    .choices_evaluated
                    .fetch_add(1, Ordering::Relaxed);
            }
            DecisionOutcome::Score { .. } => {
                self.counters
                    .scores_evaluated
                    .fetch_add(1, Ordering::Relaxed);
            }
            DecisionOutcome::YesNo { .. } => {
                self.counters
                    .yesno_evaluated
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        let id = req
            .id
            .unwrap_or_else(|| format!("dec-{}", uuid::Uuid::new_v4()));

        Ok(DecisionResponse {
            id,
            outcome,
            latency_us: elapsed_us,
        })
    }

    /// Access to real-time decision counters.
    pub fn counters(&self) -> &DecisionCounters {
        &self.counters
    }
}
