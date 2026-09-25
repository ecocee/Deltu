//! The action layer: executes what rules request.
//!
//! `ActionDispatcher` runs each [`ActionRequest`] to completion, collects
//! every result, and never propagates a failure to the caller — a failed
//! external action cannot crash the runtime (architecture invariant 8).
//! Outcomes and counters are honest: failures are observable, never
//! hidden behind fake success.

pub mod error;
pub mod log_action;

use std::time::Instant;

use serde::{Deserialize, Serialize};

pub use error::{ActionConfigError, ActionError};
pub use log_action::LogLevel;

use crate::rules::ActionRequest;

/// What to execute for a configured action id. Network transports
/// (webhook, MQTT publish) implement this same trait when they land with
/// the async runtime in Units 07/08 — dispatch semantics do not change.
pub trait ActionExecutor: std::fmt::Debug + Send {
    /// Executes one request to completion. Implementations must not panic
    /// on plausible external failures; returned errors become outcomes.
    fn execute(
        &mut self,
        request: &ActionRequest,
        now_ms: i64,
    ) -> Result<ActionSummary, ActionError>;
}

/// The successful result of one action execution.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionSummary {
    /// Human-readable detail (for the log action: the rendered line).
    pub detail: String,
}

/// The behavior of a configured action. Network transports are defined
/// here but become executable in Units 07/08 (documented build-order
/// refinement: no HTTP client crate before the async runtime exists).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionKind {
    /// Writes one structured JSON line per execution.
    Log {
        /// Severity of the emitted line.
        level: LogLevel,
        /// Optional message template with `{rule_id}` / `{payload}`
        /// placeholders.
        template: Option<String>,
    },
}

/// A configured action: its id plus its behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDefinition {
    /// Unique, non-empty action id (rules reference this).
    pub id: String,
    /// What the action does.
    pub kind: ActionKind,
}

/// The result of one dispatched request: identity, outcome, and timing.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutcome {
    /// Action that was attempted.
    pub action_id: String,
    /// Rule that produced the request.
    pub rule_id: String,
    /// `Ok(summary)` or the failure reason — collected, never propagated.
    pub result: Result<ActionSummary, ActionError>,
    /// Wall-cost of the execution in milliseconds (monotonic measurement;
    /// observability only).
    pub elapsed_ms: u64,
}

/// Execution counters — failures must be observable, never hidden
/// (code standards; same pattern as `PipelineCounters`/`StateCounters`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionCounters {
    /// Requests dispatched.
    pub actions_attempted: u64,
    /// Executions that returned `Ok`.
    pub actions_succeeded: u64,
    /// Executions that returned `Err` (including unknown action ids).
    pub actions_failed: u64,
}

/// Executes rule-produced requests with failure isolation.
#[derive(Debug)]
pub struct ActionDispatcher {
    executors: Vec<(String, Box<dyn ActionExecutor>)>,
    counters: ActionCounters,
}

impl ActionDispatcher {
    /// Validates the definitions and builds one executor per action.
    pub fn new(definitions: Vec<ActionDefinition>) -> Result<Self, ActionConfigError> {
        let mut executors: Vec<(String, Box<dyn ActionExecutor>)> =
            Vec::with_capacity(definitions.len());

        for definition in definitions {
            let id = definition.id.trim().to_string();
            if id.is_empty() {
                return Err(ActionConfigError::EmptyActionId(id));
            }
            if executors.iter().any(|(existing, _)| *existing == id) {
                return Err(ActionConfigError::DuplicateActionId(id));
            }

            let executor: Box<dyn ActionExecutor> = match definition.kind {
                ActionKind::Log { level, template } => Box::new(LogExecutor { level, template }),
            };
            executors.push((id, executor));
        }

        Ok(Self {
            executors,
            counters: ActionCounters::default(),
        })
    }

    /// Dispatches every request in order, collecting one outcome per
    /// request. A failing action yields a failed outcome — never a panic,
    /// never an abort of the remaining requests (invariant 8).
    pub fn dispatch(&mut self, requests: &[ActionRequest], now_ms: i64) -> Vec<ActionOutcome> {
        let mut outcomes = Vec::with_capacity(requests.len());
        for request in requests {
            self.counters.actions_attempted += 1;
            let started = Instant::now();

            let result = match self.executor_for(&request.action) {
                Some(executor) => executor.execute(request, now_ms),
                None => Err(ActionError::UnknownAction(request.action.clone())),
            };

            match &result {
                Ok(_) => self.counters.actions_succeeded += 1,
                Err(_) => self.counters.actions_failed += 1,
            }

            outcomes.push(ActionOutcome {
                action_id: request.action.clone(),
                rule_id: request.rule_id.clone(),
                result,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
        }
        outcomes
    }

    /// Counter snapshot.
    pub fn counters(&self) -> &ActionCounters {
        &self.counters
    }

    fn executor_for(&mut self, action_id: &str) -> Option<&mut Box<dyn ActionExecutor>> {
        self.executors
            .iter_mut()
            .find(|(id, _)| id == action_id)
            .map(|(_, executor)| executor)
    }
}

/// The built-in log executor: renders the structured line and writes it to
/// stderr.
#[derive(Debug)]
struct LogExecutor {
    level: LogLevel,
    template: Option<String>,
}

impl ActionExecutor for LogExecutor {
    fn execute(
        &mut self,
        request: &ActionRequest,
        now_ms: i64,
    ) -> Result<ActionSummary, ActionError> {
        let line = log_action::render_line(
            &request.rule_id,
            &request.action,
            self.level,
            &request.payload,
            now_ms,
            self.template.as_deref(),
        );
        // stderr: diagnostics belong there; stdout stays for program output.
        eprintln!("{line}");
        Ok(ActionSummary { detail: line })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(rule_id: &str, action_id: &str) -> ActionRequest {
        ActionRequest {
            rule_id: rule_id.to_string(),
            action: action_id.to_string(),
            payload: json!({ "mean": 85.0 }),
        }
    }

    fn log_def(id: &str) -> ActionDefinition {
        ActionDefinition {
            id: id.to_string(),
            kind: ActionKind::Log {
                level: LogLevel::Warn,
                template: None,
            },
        }
    }

    /// A deliberately-failing executor (spec 06: failure-isolation tests
    /// require one).
    #[derive(Debug)]
    struct FailingExecutor;
    impl ActionExecutor for FailingExecutor {
        fn execute(
            &mut self,
            _request: &ActionRequest,
            _now_ms: i64,
        ) -> Result<ActionSummary, ActionError> {
            Err(ActionError::ExecutionFailed {
                action_id: "will-fail".to_string(),
                reason: "external system rejected the request".to_string(),
            })
        }
    }

    // --- Dispatcher behavior ---

    #[test]
    fn log_action_execution_succeeds_with_rendered_line() {
        let mut dispatcher = ActionDispatcher::new(vec![log_def("log-ops")]).unwrap();
        let outcomes = dispatcher.dispatch(&[request("hot-room", "log-ops")], 1_000);

        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].result.is_ok());
        let summary = outcomes[0].result.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&summary.detail).unwrap();
        assert_eq!(parsed["rule_id"], "hot-room");
        assert_eq!(parsed["payload"]["mean"], 85.0);
        assert_eq!(dispatcher.counters().actions_succeeded, 1);
        assert_eq!(dispatcher.counters().actions_failed, 0);
    }

    #[test]
    fn failing_action_yields_failed_outcome_without_panicking() {
        // A failing executor registered under a valid id: dispatch must
        // collect the error as an outcome (invariant 8 pattern).
        let mut dispatcher = ActionDispatcher {
            executors: vec![("will-fail".to_string(), Box::new(FailingExecutor))],
            counters: ActionCounters::default(),
        };

        let outcomes = dispatcher.dispatch(&[request("hot-room", "will-fail")], 1_000);
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0].result {
            Err(ActionError::ExecutionFailed { action_id, .. }) => {
                assert_eq!(action_id, "will-fail")
            }
            other => panic!("expected execution failure, got {other:?}"),
        }
        assert_eq!(dispatcher.counters().actions_failed, 1);
        assert_eq!(dispatcher.counters().actions_succeeded, 0);
    }

    #[test]
    fn one_failing_action_does_not_abort_remaining_requests() {
        let mut dispatcher = ActionDispatcher {
            executors: vec![
                (
                    "log-ops".to_string(),
                    Box::new(LogExecutor {
                        level: LogLevel::Info,
                        template: None,
                    }),
                ),
                ("will-fail".to_string(), Box::new(FailingExecutor)),
            ],
            counters: ActionCounters::default(),
        };

        let requests = vec![
            request("r1", "log-ops"),
            request("r2", "will-fail"),
            request("r3", "log-ops"),
        ];
        let outcomes = dispatcher.dispatch(&requests, 1_000);

        assert_eq!(outcomes.len(), 3); // all attempted
        assert!(outcomes[0].result.is_ok());
        assert!(outcomes[1].result.is_err());
        assert!(outcomes[2].result.is_ok()); // dispatch continued past failure
        assert_eq!(dispatcher.counters().actions_attempted, 3);
        assert_eq!(dispatcher.counters().actions_succeeded, 2);
        assert_eq!(dispatcher.counters().actions_failed, 1);
    }

    #[test]
    fn unknown_action_id_is_an_error_outcome_not_a_panic() {
        let mut dispatcher = ActionDispatcher::new(vec![log_def("log-ops")]).unwrap();
        let outcomes = dispatcher.dispatch(&[request("r", "no-such-action")], 1_000);
        assert_eq!(
            outcomes[0].result,
            Err(ActionError::UnknownAction("no-such-action".to_string()))
        );
        assert_eq!(dispatcher.counters().actions_failed, 1);
    }

    #[test]
    fn outcome_order_matches_request_order() {
        let mut dispatcher = ActionDispatcher::new(vec![log_def("a1"), log_def("a2")]).unwrap();
        let requests = vec![request("r", "a2"), request("r", "a1")];
        let outcomes = dispatcher.dispatch(&requests, 1_000);
        assert_eq!(outcomes[0].action_id, "a2");
        assert_eq!(outcomes[1].action_id, "a1");
    }

    // --- Config errors ---

    #[test]
    fn empty_action_id_is_rejected() {
        let err = ActionDispatcher::new(vec![ActionDefinition {
            id: "   ".to_string(),
            kind: ActionKind::Log {
                level: LogLevel::Info,
                template: None,
            },
        }])
        .unwrap_err();
        assert_eq!(err, ActionConfigError::EmptyActionId(String::new()));
    }

    #[test]
    fn duplicate_action_ids_are_rejected() {
        let err = ActionDispatcher::new(vec![log_def("dup"), log_def("dup")]).unwrap_err();
        assert_eq!(err, ActionConfigError::DuplicateActionId("dup".to_string()));
    }
}
