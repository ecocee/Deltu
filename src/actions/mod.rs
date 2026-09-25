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

/// The behavior of a configured action.
///
/// The `Ai` variant (spec 11) routes the rule-produced request through the
/// `AiManager` — the only path by which any AI provider is reachable
/// (invocation policy enforced by construction). Without a configured
/// provider, `Ai` requests fail fast as counted outcomes.
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
    /// Sends the rule snapshot to the configured AI provider.
    Ai,
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
    /// The optional AI layer, reachable only through `ActionKind::Ai`
    /// executions (spec 11 invocation policy).
    ai: crate::ai::AiManager,
    /// Action ids whose kind is `Ai` (fixed at construction).
    ai_actions: std::collections::HashSet<String>,
}

impl ActionDispatcher {
    /// Replaces the AI layer (used by the runtime when configuration
    /// enables a provider).
    pub fn set_ai_manager(&mut self, ai: crate::ai::AiManager) {
        self.ai = ai;
    }

    /// Read-only AI usage snapshot for `/v1/status`.
    pub fn ai_usage(&self) -> crate::ai::AiUsage {
        *self.ai.usage()
    }
}

impl ActionDispatcher {
    /// Validates the definitions and builds one executor per action.
    pub fn new(definitions: Vec<ActionDefinition>) -> Result<Self, ActionConfigError> {
        let mut executors: Vec<(String, Box<dyn ActionExecutor>)> =
            Vec::with_capacity(definitions.len());

        let ai_actions: std::collections::HashSet<String> = definitions
            .iter()
            .filter(|definition| matches!(definition.kind, ActionKind::Ai))
            .map(|definition| definition.id.trim().to_string())
            .collect();

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
                // AI executions route through the AI manager at dispatch
                // time (spec 11 policy: the manager is the only provider
                // path).
                ActionKind::Ai => Box::new(AiExecutor),
            };
            executors.push((id, executor));
        }

        Ok(Self {
            executors,
            counters: ActionCounters::default(),
            ai: crate::ai::AiManager::disabled(),
            ai_actions,
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

            // Spec 11 invocation policy: `ActionKind::Ai` routes through
            // the AI manager — the only path by which any provider is
            // reachable. Its outcome converts to an honest action outcome.
            let is_ai = self.ai_actions.contains(&request.action);
            let result = if is_ai {
                let ai_request = crate::ai::request_from_action(request, now_ms);
                match self.ai.handle(&ai_request) {
                    crate::ai::AiOutcome::Completed(result) => Ok(ActionSummary {
                        detail: result.output.to_string(),
                    }),
                    crate::ai::AiOutcome::Failed(error) => Err(ActionError::ExecutionFailed {
                        action_id: request.action.clone(),
                        reason: error.to_string(),
                    }),
                }
            } else {
                match self.executor_for(&request.action) {
                    Some(executor) => executor.execute(request, now_ms),
                    None => Err(ActionError::UnknownAction(request.action.clone())),
                }
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

/// Marker executor for `ActionKind::Ai`; dispatch routes these requests
/// through the AI manager (the policy boundary).
#[derive(Debug)]
struct AiExecutor;

impl ActionExecutor for AiExecutor {
    fn execute(
        &mut self,
        _request: &ActionRequest,
        _now_ms: i64,
    ) -> Result<ActionSummary, ActionError> {
        // Placeholder: real handling happens in dispatch (see below),
        // which consults the AI manager. This arm exists so definition
        // parsing is total.
        Ok(ActionSummary {
            detail: "ai action handled by the dispatcher".to_string(),
        })
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
            ai: crate::ai::AiManager::disabled(),
            ai_actions: std::collections::HashSet::new(),
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
            ai: crate::ai::AiManager::disabled(),
            ai_actions: std::collections::HashSet::new(),
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

    // --- AI policy (spec 11) ---

    fn ai_def(id: &str) -> ActionDefinition {
        ActionDefinition {
            id: id.to_string(),
            kind: ActionKind::Ai,
        }
    }

    #[test]
    fn ai_action_routes_through_manager_and_counts_usage() {
        let mut dispatcher = ActionDispatcher::new(vec![ai_def("ai-triage")]).unwrap();
        dispatcher.set_ai_manager(crate::ai::AiManager::with_provider(Box::new(
            crate::ai::ScriptedProvider::new(
                "test-model",
                vec![Ok((json!({ "triage": "critical" }), 42))],
            ),
        )));

        let outcomes = dispatcher.dispatch(&[request("hot-room", "ai-triage")], 1_000);
        assert!(outcomes[0].result.is_ok());
        let summary = outcomes[0].result.as_ref().unwrap();
        assert!(summary.detail.contains("critical"));

        let usage = dispatcher.ai_usage();
        assert_eq!(usage.calls, 1);
        assert_eq!(usage.calls_succeeded, 1);
        assert_eq!(usage.tokens_used, 42);
    }

    #[test]
    fn ai_action_without_provider_fails_fast_as_counted_outcome() {
        let mut dispatcher = ActionDispatcher::new(vec![ai_def("ai-triage")]).unwrap();

        let outcomes = dispatcher.dispatch(&[request("hot-room", "ai-triage")], 1_000);
        assert!(outcomes[0].result.is_err());
        assert_eq!(dispatcher.ai_usage().calls, 1);
        assert_eq!(dispatcher.ai_usage().calls_failed, 1);
    }

    #[test]
    fn policy_ai_manager_is_unreachable_without_an_ai_action() {
        // A log action must never touch the AI manager even when one is
        // configured — the provider path exists only for `ActionKind::Ai`.
        let mut dispatcher = ActionDispatcher::new(vec![log_def("log-ops")]).unwrap();
        dispatcher.set_ai_manager(crate::ai::AiManager::with_provider(Box::new(
            crate::ai::ScriptedProvider::new("test-model", vec![]),
        )));

        let outcomes = dispatcher.dispatch(&[request("r", "log-ops")], 1_000);
        assert!(outcomes[0].result.is_ok());
        assert_eq!(dispatcher.ai_usage().calls, 0); // untouched
    }
}
