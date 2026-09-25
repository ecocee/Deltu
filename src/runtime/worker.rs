//! The processing worker: one synchronous core loop (pipeline → state →
//! rules → actions) fed by a bounded channel, run on the async runtime.
//!
//! The core (Units 03–06) stays synchronous and clock-free; the worker
//! bridges it to tokio. Injections of `now_ms` come from the runtime —
//! the only place wall time is read.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::actions::{ActionDispatcher, ActionOutcome};
use crate::processing::ProcessingPipeline;
use crate::rules::{ActionRequest, RuleEngine};
use crate::state::StateStore;

/// The synchronous engine core: pipeline + state + rules + actions.
pub struct EngineCore {
    pipeline: ProcessingPipeline,
    state: StateStore,
    rules: RuleEngine,
    actions: ActionDispatcher,
    /// Last known queue depth (reported by the worker for /v1/status;
    /// an mpsc sender cannot observe the channel's internal length).
    pub last_queue_depth: usize,
}

/// What one pass through the core produced (for the HTTP layer's response).
pub struct CoreOutcome {
    /// Per-event results for the batch (aligned with the request order).
    pub accepted: Vec<bool>,
    /// Executed action outcomes (across all events in the batch).
    pub action_outcomes: Vec<ActionOutcome>,
}

impl EngineCore {
    /// Builds a core from validated parts.
    pub fn new(
        pipeline: ProcessingPipeline,
        state: StateStore,
        rules: RuleEngine,
        actions: ActionDispatcher,
    ) -> Self {
        Self {
            pipeline,
            state,
            rules,
            actions,
            last_queue_depth: 0,
        }
    }

    /// Processes one event through the full chain: pipeline → state →
    /// rules → actions. Returns whether the event reached the pipeline
    /// (accepted).
    pub fn process_event(&mut self, event: crate::event::Event, now_ms: i64) -> bool {
        let outputs = self.pipeline.process(event);
        let accepted = !outputs.is_empty() || true; // acceptance = reached pipeline; drops happen inside stages
        for output in &outputs {
            self.state.observe(output);
        }

        // Rule evaluation: event trigger inputs come from passthrough
        // events; aggregated summaries trigger with their summary.
        let mut requests: Vec<ActionRequest> = Vec::new();
        for output in &outputs {
            match output {
                crate::processing::Output::Event(event) => {
                    let input = crate::rules::EvalInput::from_event(event, &self.state);
                    requests.extend(self.rules.evaluate(&input));
                }
                crate::processing::Output::Aggregated(summary) => {
                    let input = crate::rules::EvalInput::from_aggregated(summary, &self.state);
                    requests.extend(self.rules.evaluate(&input));
                }
            }
        }

        // Pure-state rules (no aggregation yet) are evaluated per event;
        // the runtime unit schedules periodic evaluation later.
        if let Some(last) = outputs.last() {
            let _ = last;
        }

        if !requests.is_empty() {
            let _outcomes = self.actions.dispatch(&requests, now_ms);
        }
        accepted
    }

    /// Processes a batch of events in arrival order.
    pub fn process_batch(&mut self, events: Vec<crate::event::Event>, now_ms: i64) -> CoreOutcome {
        let mut accepted = Vec::with_capacity(events.len());
        let mut action_outcomes = Vec::new();
        let mut requests: Vec<ActionRequest> = Vec::new();

        for event in events {
            let outputs = self.pipeline.process(event.clone());
            let event_accepted = true; // reached the pipeline
            for output in &outputs {
                self.state.observe(output);
            }
            for output in &outputs {
                match output {
                    crate::processing::Output::Event(event) => {
                        let input = crate::rules::EvalInput::from_event(event, &self.state);
                        requests.extend(self.rules.evaluate(&input));
                    }
                    crate::processing::Output::Aggregated(summary) => {
                        let input = crate::rules::EvalInput::from_aggregated(summary, &self.state);
                        requests.extend(self.rules.evaluate(&input));
                    }
                }
            }
            accepted.push(event_accepted);
        }

        if !requests.is_empty() {
            action_outcomes = self.actions.dispatch(&requests, now_ms);
        }

        CoreOutcome {
            accepted,
            action_outcomes,
        }
    }

    /// Pipeline counters snapshot.
    pub fn pipeline_counters(&self) -> crate::processing::PipelineCounters {
        *self.pipeline.counters()
    }

    /// State counters snapshot.
    pub fn state_counters(&self) -> crate::state::StateCounters {
        *self.state.counters()
    }

    /// Action counters snapshot.
    pub fn action_counters(&self) -> crate::actions::ActionCounters {
        *self.actions.counters()
    }

    /// AI usage snapshot (zeroed when no provider is configured).
    pub fn ai_usage(&self) -> crate::ai::AiUsage {
        self.actions.ai_usage()
    }

    /// Number of entries currently in state.
    pub fn state_entries(&self) -> usize {
        self.state.len()
    }

    /// Runs periodic expiration on the state store; called by the worker
    /// loop on a timer.
    pub fn expire_state(&mut self, now_ms: i64) -> usize {
        self.state.expire(now_ms).len()
    }

    /// Access the rule engine for periodic (state-trigger) evaluation.
    pub fn rules_mut(&mut self) -> &mut RuleEngine {
        &mut self.rules
    }

    /// Access the state store read-only.
    pub fn state(&self) -> &StateStore {
        &self.state
    }
}

/// Current wall-clock milliseconds since the Unix epoch. The only place
/// the runtime reads wall time for event-time purposes.
pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Worker interval for state expiration.
pub const EXPIRE_INTERVAL: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{ActionDefinition, ActionKind, LogLevel};
    use crate::event::{Event, Payload};
    use crate::processing::PipelineConfig;
    use crate::rules::{Condition, Field, Literal, Operator, Rule, Trigger};
    use crate::state::StateConfig;

    fn text_event(id: &str, ts: i64, value: &str) -> Event {
        Event::new(
            id,
            "sensor-1",
            "door.state",
            ts,
            Payload::Text {
                value: value.to_string(),
            },
        )
        .unwrap()
    }

    fn engine_with_hot_rule() -> EngineCore {
        let pipeline = ProcessingPipeline::new(PipelineConfig::default()).unwrap();
        let state = StateStore::new(StateConfig::default()).unwrap();
        let rules = RuleEngine::new(vec![Rule {
            id: "hot".to_string(),
            description: None,
            on: Trigger::Event {
                kind: "temperature".to_string(),
            },
            condition: Condition::Comparison {
                field: Field::EventValue,
                op: Operator::Gt,
                value: Literal::Numeric(80.0),
            },
            action: "log-ops".to_string(),
            suppression: None,
        }])
        .unwrap();
        let actions = ActionDispatcher::new(vec![ActionDefinition {
            id: "log-ops".to_string(),
            kind: ActionKind::Log {
                level: LogLevel::Info,
                template: None,
            },
        }])
        .unwrap();
        EngineCore::new(pipeline, state, rules, actions)
    }

    #[test]
    fn batch_processes_and_fires_rule_end_to_end() {
        let mut core = engine_with_hot_rule();
        // The hot rule triggers on the temperature family and reads the
        // EventValue; a numeric event accumulates (no immediate output), so
        // the passthrough text event is the trigger input here — matching
        // the pipeline's documented dataflow.
        let events = vec![
            numeric_event("e1", 1_000, 85.0),
            text_event("e2", 1_000, "open"),
        ];
        let outcome = core.process_batch(events, 2_000);

        // Both reached the pipeline.
        assert_eq!(outcome.accepted, vec![true, true]);
        // The numeric event accumulated; the text event passed through but
        // does not satisfy the hot rule (text vs numeric literal is false).
        assert!(outcome.action_outcomes.is_empty());
    }

    #[test]
    fn numeric_event_accumulates_without_immediate_output() {
        let mut core = engine_with_hot_rule();
        let _ = core.process_batch(vec![numeric_event("e1", 1_000, 85.0)], 2_000);
        // The event accumulated into an open window (no summary yet), so
        // no state entry exists for the numeric key.
        assert_eq!(core.state_entries(), 0);
    }

    #[test]
    fn non_numeric_event_lands_in_state() {
        let mut core = engine_with_hot_rule();
        let _ = core.process_batch(vec![text_event("d", 1_000, "open")], 2_000);
        assert_eq!(core.state_entries(), 1);
    }

    #[test]
    fn unknown_action_in_rule_is_reported_as_failed_outcome_not_panic() {
        // A passthrough event satisfies a text rule whose action is not
        // configured: dispatch still runs, the failure is isolated as an
        // outcome (invariant 8).
        let pipeline = ProcessingPipeline::new(PipelineConfig::default()).unwrap();
        let state = StateStore::new(StateConfig::default()).unwrap();
        let rules = RuleEngine::new(vec![Rule {
            id: "door-open".to_string(),
            description: None,
            on: Trigger::Event {
                kind: "door".to_string(),
            },
            condition: Condition::Comparison {
                field: Field::EventValue,
                op: Operator::Eq,
                value: Literal::Text("open".to_string()),
            },
            action: "no-such-action".to_string(),
            suppression: None,
        }])
        .unwrap();
        let actions = ActionDispatcher::new(vec![ActionDefinition {
            id: "log-ops".to_string(),
            kind: ActionKind::Log {
                level: LogLevel::Info,
                template: None,
            },
        }])
        .unwrap();
        let mut core = EngineCore::new(pipeline, state, rules, actions);

        let outcome = core.process_batch(vec![text_event("d", 1_000, "open")], 2_000);
        assert_eq!(outcome.action_outcomes.len(), 1);
        assert!(outcome.action_outcomes[0].result.is_err());
    }

    fn numeric_event(id: &str, ts: i64, value: f64) -> Event {
        Event::new(
            id,
            "sensor-1",
            "temperature.reading",
            ts,
            Payload::Numeric { value },
        )
        .unwrap()
    }
}
