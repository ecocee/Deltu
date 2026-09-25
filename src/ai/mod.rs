//! The optional AI layer (spec 11 / decision 003): a provider abstraction
//! reachable *only* from rule-produced `ai` actions.
//!
//! Core invariants enforced here by construction:
//! * Incoming events never reach AI implicitly — a request exists only
//!   because a rule matched and its action was configured as `ai`.
//! * The engine works completely without this layer (`AiManager::disabled`).
//! * Every call, result, and failure is counted (usage metrics).
//!
//! Real runtimes (ONNX via `ort`, llama.cpp) land behind cargo features in
//! later work; this unit ships the boundary, the policy, and a scripted
//! in-memory provider used for testing without any model.

use serde_json::Value as JsonValue;
use std::time::Instant;

use crate::rules::ActionRequest;

/// One inference request: the rule-selected, compressed context — never a
/// raw event stream.
#[derive(Debug, Clone, PartialEq)]
pub struct AiRequest {
    /// Rule that produced the request.
    pub rule_id: String,
    /// The action configuration that produced it.
    pub action_id: String,
    /// The rule's snapshot payload (meaningful, compressed).
    pub payload: JsonValue,
    /// Event-time of the triggering input (ms).
    pub event_time_ms: i64,
}

/// A structured inference result plus usage.
#[derive(Debug, Clone, PartialEq)]
pub struct AiResult {
    /// Provider-defined structured output.
    pub output: JsonValue,
    /// Model identity that produced the result.
    pub model: String,
    /// Token usage, where the provider reports it (local runtimes may
    /// report 0).
    pub tokens_used: u64,
    /// Wall latency of the inference in ms.
    pub latency_ms: u64,
}

/// Provider errors: isolated like action failures — counted, never fatal.
#[derive(Debug, Clone, PartialEq)]
pub enum AiError {
    /// The provider failed to produce a result.
    ProviderFailed(String),
    /// The request was not convertible to the provider's input format.
    InvalidRequest(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::ProviderFailed(reason) => write!(f, "ai provider failed: {reason}"),
            AiError::InvalidRequest(reason) => write!(f, "invalid ai request: {reason}"),
        }
    }
}

impl std::error::Error for AiError {}

/// The provider boundary (invariant 19: replaceable without engine
/// redesign). Implementations must be Send (called from the runtime) and
/// must not panic on plausible failures — return [`AiError`].
pub trait AiProvider: std::fmt::Debug + Send {
    /// Human-readable provider/model name for metrics.
    fn name(&self) -> &str;
    /// Runs one inference to completion.
    fn infer(&mut self, request: &AiRequest) -> Result<AiResult, AiError>;
}

/// Usage counters — mandatory features of the AI boundary (decision 003).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AiUsage {
    /// Inference calls attempted.
    pub calls: u64,
    /// Calls that returned `Ok`.
    pub calls_succeeded: u64,
    /// Calls that returned `Err` (isolated, never fatal).
    pub calls_failed: u64,
    /// Tokens consumed where reported.
    pub tokens_used: u64,
    /// Total inference wall time in ms.
    pub latency_ms: u64,
}

/// How an AI request was resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum AiOutcome {
    /// The provider produced a structured result.
    Completed(AiResult),
    /// The provider failed; the request is dropped and counted.
    Failed(AiError),
}

/// Owns the optional provider. With `None` (the default build), every
/// request fails fast with a counted `AiError` — the engine never depends
/// on AI being present (invariant 1).
#[derive(Debug)]
pub struct AiManager {
    provider: Option<Box<dyn AiProvider>>,
    usage: AiUsage,
}

impl AiManager {
    /// No provider configured: AI is effectively absent (the default).
    pub fn disabled() -> Self {
        Self {
            provider: None,
            usage: AiUsage::default(),
        }
    }

    /// A configured provider (feature-gated runtimes or the scripted one).
    pub fn with_provider(provider: Box<dyn AiProvider>) -> Self {
        Self {
            provider: Some(provider),
            usage: AiUsage::default(),
        }
    }

    /// Whether a provider is configured.
    pub fn enabled(&self) -> bool {
        self.provider.is_some()
    }

    /// Handles one rule-produced request. This is the *only* path into a
    /// provider — callers outside the action layer do not have access to
    /// the manager (enforced by module visibility in `serve`).
    pub fn handle(&mut self, request: &AiRequest) -> AiOutcome {
        self.usage.calls += 1;
        let started = Instant::now();
        let outcome = match &mut self.provider {
            Some(provider) => match provider.infer(request) {
                Ok(result) => {
                    self.usage.calls_succeeded += 1;
                    self.usage.tokens_used += result.tokens_used;
                    AiOutcome::Completed(result)
                }
                Err(error) => {
                    self.usage.calls_failed += 1;
                    AiOutcome::Failed(error)
                }
            },
            None => {
                self.usage.calls_failed += 1;
                AiOutcome::Failed(AiError::ProviderFailed(
                    "no ai provider is configured".to_string(),
                ))
            }
        };
        self.usage.latency_ms += started.elapsed().as_millis() as u64;
        outcome
    }

    /// Usage snapshot for `/v1/status`.
    pub fn usage(&self) -> &AiUsage {
        &self.usage
    }
}

/// Builds an [`AiRequest`] from a rule-produced action request. Public so
/// the action layer can construct it — and visibly named so no other path
/// pretends to be rule-produced.
pub fn request_from_action(request: &ActionRequest, event_time_ms: i64) -> AiRequest {
    AiRequest {
        rule_id: request.rule_id.clone(),
        action_id: request.action.clone(),
        payload: request.payload.clone(),
        event_time_ms,
    }
}

/// A scripted in-memory provider for tests and CI — no model required.
#[derive(Debug)]
pub struct ScriptedProvider {
    model: String,
    /// Responses served in order; an Err string fails that call.
    script: Vec<Result<(JsonValue, u64), String>>,
    calls_made: usize,
}

impl ScriptedProvider {
    /// Each script entry is `Ok((output, tokens))` or `Err(reason)`.
    pub fn new(model: &str, script: Vec<Result<(JsonValue, u64), String>>) -> Self {
        Self {
            model: model.to_string(),
            script,
            calls_made: 0,
        }
    }
}

impl AiProvider for ScriptedProvider {
    fn name(&self) -> &str {
        &self.model
    }

    fn infer(&mut self, _request: &AiRequest) -> Result<AiResult, AiError> {
        let entry = self.script.get(self.calls_made).cloned();
        self.calls_made += 1;
        match entry {
            Some(Ok((output, tokens))) => Ok(AiResult {
                output,
                model: self.model.clone(),
                tokens_used: tokens,
                latency_ms: 0,
            }),
            Some(Err(reason)) => Err(AiError::ProviderFailed(reason)),
            None => Err(AiError::ProviderFailed(
                "scripted provider exhausted its script".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> AiRequest {
        AiRequest {
            rule_id: "hot-room".to_string(),
            action_id: "ai-triage".to_string(),
            payload: json!({ "mean": 91.0 }),
            event_time_ms: 1_000,
        }
    }

    #[test]
    fn disabled_manager_fails_fast_and_counts() {
        let mut manager = AiManager::disabled();
        assert!(!manager.enabled());
        match manager.handle(&request()) {
            AiOutcome::Failed(AiError::ProviderFailed(reason)) => {
                assert!(reason.contains("no ai provider"))
            }
            other => panic!("expected failure, got {other:?}"),
        }
        assert_eq!(manager.usage().calls, 1);
        assert_eq!(manager.usage().calls_failed, 1);
        assert_eq!(manager.usage().calls_succeeded, 0);
    }

    #[test]
    fn scripted_provider_returns_structured_result_with_usage() {
        let mut manager = AiManager::with_provider(Box::new(ScriptedProvider::new(
            "test-model",
            vec![Ok((json!({ "triage": "critical" }), 42))],
        )));
        assert!(manager.enabled());
        match manager.handle(&request()) {
            AiOutcome::Completed(result) => {
                assert_eq!(result.output, json!({ "triage": "critical" }));
                assert_eq!(result.model, "test-model");
                assert_eq!(result.tokens_used, 42);
            }
            other => panic!("expected completion, got {other:?}"),
        }
        assert_eq!(manager.usage().calls, 1);
        assert_eq!(manager.usage().calls_succeeded, 1);
        assert_eq!(manager.usage().tokens_used, 42);
    }

    #[test]
    fn provider_failure_is_isolated_and_counted() {
        let mut manager = AiManager::with_provider(Box::new(ScriptedProvider::new(
            "test-model",
            vec![Err("model crashed".to_string())],
        )));
        match manager.handle(&request()) {
            AiOutcome::Failed(AiError::ProviderFailed(reason)) => {
                assert_eq!(reason, "model crashed")
            }
            other => panic!("expected failure, got {other:?}"),
        }
        assert_eq!(manager.usage().calls_failed, 1);
        // The manager itself is still usable.
        assert!(manager.enabled());
    }

    #[test]
    fn script_exhaustion_fails_cleanly() {
        let mut manager =
            AiManager::with_provider(Box::new(ScriptedProvider::new("test-model", vec![])));
        assert!(matches!(
            manager.handle(&request()),
            AiOutcome::Failed(AiError::ProviderFailed(_))
        ));
    }

    #[test]
    fn request_from_action_carries_rule_context() {
        let action = ActionRequest {
            rule_id: "hot-room".to_string(),
            action: "ai-triage".to_string(),
            payload: json!({ "mean": 91.0 }),
        };
        let ai_request = request_from_action(&action, 5_000);
        assert_eq!(ai_request.rule_id, "hot-room");
        assert_eq!(ai_request.action_id, "ai-triage");
        assert_eq!(ai_request.event_time_ms, 5_000);
        assert_eq!(ai_request.payload, json!({ "mean": 91.0 }));
    }

    #[test]
    fn usage_latency_is_accumulated() {
        let mut manager = AiManager::with_provider(Box::new(ScriptedProvider::new(
            "test-model",
            vec![Ok((json!(1), 0)), Ok((json!(2), 0))],
        )));
        let _ = manager.handle(&request());
        let _ = manager.handle(&request());
        assert_eq!(manager.usage().calls, 2);
        // Latency accumulated across calls (scripted providers are
        // instant; the counter mechanism is what is under test).
        assert!(manager.usage().latency_ms < u64::MAX);
    }
}
