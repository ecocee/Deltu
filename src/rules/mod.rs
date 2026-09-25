//! Deltu's deterministic rule engine: typed conditions evaluated against
//! pipeline inputs and current state, producing action requests.
//!
//! Pure, synchronous, total — evaluation never errors (missing state or
//! type mismatches compare `false`), and output is deterministic
//! (matches sorted by rule id). No AI, no I/O, no async.

pub mod definition;
pub mod error;

use std::collections::HashMap;

use crate::event::Event;
use crate::processing::Aggregated;
use crate::state::StateStore;

pub use definition::{Condition, Field, Literal, OncePerWindow, Operator, Rule, Stat, Trigger};
pub use error::RuleConfigError;

/// What the rule engine evaluates: the triggering input (a raw event, a
/// window summary, or neither — for pure state rules) plus the current
/// state to read through Unit 04's interface only.
pub struct EvalInput<'a> {
    /// The triggering raw event, when the input was one.
    pub event: Option<&'a Event>,
    /// The triggering window summary, when the input was one.
    pub aggregated: Option<&'a Aggregated>,
    /// Current runtime state.
    pub state: &'a StateStore,
}

impl<'a> EvalInput<'a> {
    /// Builds an input from a raw event.
    pub fn from_event(event: &'a Event, state: &'a StateStore) -> Self {
        Self {
            event: Some(event),
            aggregated: None,
            state,
        }
    }

    /// Builds an input from a window summary.
    pub fn from_aggregated(aggregated: &'a Aggregated, state: &'a StateStore) -> Self {
        Self {
            event: None,
            aggregated: Some(aggregated),
            state,
        }
    }
}

/// A rule that fired, with the payload an action will receive.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleMatch {
    /// The rule that fired.
    pub rule_id: String,
    /// The action id to request (resolved by Unit 06's dispatcher).
    pub action: String,
    /// Meaningful, compressed context for the action: rule id, trigger
    /// kind, the value that satisfied the condition where applicable, and
    /// the event-time of the input.
    pub snapshot: serde_json::Value,
}

/// An action request produced by a fired rule — the input Unit 06's
/// dispatcher consumes.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionRequest {
    /// Rule that produced the request.
    pub rule_id: String,
    /// Action to execute.
    pub action: String,
    /// Compressed context payload for the action.
    pub payload: serde_json::Value,
}

impl From<RuleMatch> for ActionRequest {
    fn from(matched: RuleMatch) -> Self {
        Self {
            rule_id: matched.rule_id,
            action: matched.action,
            payload: matched.snapshot,
        }
    }
}

/// The engine: validated rules in id order, plus one suppression slot per
/// suppressed-rule (bounded by construction — the rule set is fixed).
#[derive(Debug)]
pub struct RuleEngine {
    rules: Vec<Rule>,
    /// Per rule with suppression: the event-time window index last fired in.
    last_fired_window: HashMap<String, i64>,
    /// Count of suppressions applied (observable, spec 03/04 pattern).
    suppressed: u64,
}

impl RuleEngine {
    /// Validates the rule set and constructs the engine.
    ///
    /// Rules are stored sorted by id; evaluation is deterministic
    /// regardless of definition order.
    pub fn new(rules: Vec<Rule>) -> Result<Self, RuleConfigError> {
        let mut seen: HashMap<String, ()> = HashMap::with_capacity(rules.len());
        for rule in &rules {
            let id = rule.id.trim();
            if id.is_empty() {
                return Err(RuleConfigError::EmptyRuleId);
            }
            if seen.contains_key(id) {
                return Err(RuleConfigError::DuplicateRuleId(id.to_string()));
            }
            seen.insert(id.to_string(), ());
            if rule.action.trim().is_empty() {
                return Err(RuleConfigError::EmptyActionId);
            }
            validate_condition(&rule.id, &rule.condition)?;
            if let Some(suppression) = &rule.suppression
                && suppression.window_ms <= 0
            {
                return Err(RuleConfigError::InvalidSuppressionWindow {
                    rule_id: id.to_string(),
                    window_ms: suppression.window_ms,
                });
            }
        }

        let mut sorted = rules;
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self {
            rules: sorted,
            last_fired_window: HashMap::new(),
            suppressed: 0,
        })
    }

    /// Evaluates every rule against the input and returns action requests
    /// for fired rules, ordered by rule id.
    ///
    /// A rule fires when its trigger matches, its condition holds, and
    /// (with suppression) its event-time window has not already fired.
    pub fn evaluate(&mut self, input: &EvalInput<'_>) -> Vec<ActionRequest> {
        let input_ts = input_ts(input);
        let mut requests = Vec::new();

        for rule in &self.rules {
            if !self.trigger_matches(rule, input) {
                continue;
            }
            let Some(satisfied) = condition_value(&rule.condition, input) else {
                continue; // condition false
            };
            if !satisfied {
                continue;
            }

            let rule_id = rule.id.clone();
            if let Some(window) = &rule.suppression {
                let index = input_ts / window.window_ms;
                if self.last_fired_window.get(&rule_id) == Some(&index) {
                    self.suppressed += 1;
                    continue;
                }
                self.last_fired_window.insert(rule_id.clone(), index);
            }

            requests.push(ActionRequest {
                payload: serde_json::json!({
                    "rule_id": rule.id,
                    "rule_description": rule.description,
                    "trigger_kind": trigger_kind(rule),
                    "event_time_ms": input_ts,
                    "satisfied_value": satisfied_value(input),
                }),
                rule_id,
                action: rule.action.clone(),
            });
        }
        requests
    }

    /// Number of firings suppressed by once-per-window policies.
    pub fn suppressed(&self) -> u64 {
        self.suppressed
    }

    /// Trigger matching per the spec's finalized semantics: Event triggers
    /// match the kind of either input shape (raw event or summary) so
    /// `Field::Aggregation` conditions are reachable; State triggers are
    /// candidates on every input and fire when their key currently exists.
    fn trigger_matches(&self, rule: &Rule, input: &EvalInput<'_>) -> bool {
        match &rule.on {
            Trigger::Event { kind } => {
                if let Some(event) = input.event {
                    crate::processing::kind_matches_family(
                        &event.kind.to_lowercase(),
                        &kind.to_lowercase(),
                    )
                } else if let Some(summary) = input.aggregated {
                    crate::processing::kind_matches_family(
                        &summary.kind.to_lowercase(),
                        &kind.to_lowercase(),
                    )
                } else {
                    false
                }
            }
            Trigger::State { source, kind } => input
                .state
                .get(&crate::state::StateKey::new(source.clone(), kind.clone()))
                .is_some(),
        }
    }
}

/// Event-time of the input (summary window end for aggregated inputs),
/// defaulting to 0 when there is no time-bearing input.
fn input_ts(input: &EvalInput<'_>) -> i64 {
    if let Some(event) = input.event {
        event.timestamp
    } else if let Some(summary) = input.aggregated {
        summary.window_end_ms
    } else {
        0
    }
}

fn trigger_kind(rule: &Rule) -> String {
    match &rule.on {
        Trigger::Event { kind } => kind.clone(),
        Trigger::State { source, kind } => format!("{source}/{kind}"),
    }
}

/// The concrete value that satisfied the condition, for the action
/// snapshot (best-effort: event value, then aggregation mean).
fn satisfied_value(input: &EvalInput<'_>) -> Option<serde_json::Value> {
    if let Some(event) = input.event {
        return Some(match &event.payload {
            crate::event::Payload::Numeric { value } => serde_json::json!(value),
            crate::event::Payload::Text { value } => serde_json::json!(value),
            crate::event::Payload::Boolean { value } => serde_json::json!(value),
            crate::event::Payload::Json { value } => value.clone(),
        });
    }
    input
        .aggregated
        .map(|summary| serde_json::json!(summary.mean))
}

/// Reads the concrete value behind a field reference, if it exists.
fn field_value(field: &Field, input: &EvalInput<'_>) -> Option<FieldResolution> {
    match field {
        Field::EventValue => {
            if let Some(event) = input.event {
                Some(FieldResolution::Value(crate::state::StateValue::from(
                    &event.payload,
                )))
            } else {
                input.aggregated.map(|summary| {
                    FieldResolution::Value(crate::state::StateValue::Numeric(summary.mean))
                })
            }
        }
        Field::Aggregation { stat } => {
            let summary: &Aggregated = input.aggregated?;
            let value = match stat {
                Stat::Mean => summary.mean,
                Stat::Min => summary.min,
                Stat::Max => summary.max,
                Stat::Sum => summary.sum,
                Stat::Count => summary.count as f64,
                Stat::Last => summary.last,
            };
            Some(FieldResolution::Value(crate::state::StateValue::Numeric(
                value,
            )))
        }
        Field::State { source, kind } => Some(FieldResolution::State(crate::state::StateKey::new(
            source.clone(),
            kind.clone(),
        ))),
    }
}

/// A field either resolves to a value directly or names a state key whose
/// current entry must be looked up.
enum FieldResolution {
    Value(crate::state::StateValue),
    State(crate::state::StateKey),
}

/// Compares a resolved state value against a literal under an operator.
/// Type mismatches compare `false` — including cross-type `Ne` — so rules
/// stay deterministic and total (spec 05 finalized semantics).
fn value_satisfies(value: &crate::state::StateValue, op: &Operator, literal: &Literal) -> bool {
    match (value, literal) {
        (crate::state::StateValue::Numeric(v), Literal::Numeric(l)) => match op {
            Operator::Gt => v > l,
            Operator::Gte => v >= l,
            Operator::Lt => v < l,
            Operator::Lte => v <= l,
            Operator::Eq => v == l,
            Operator::Ne => v != l,
        },
        (crate::state::StateValue::Text(v), Literal::Text(l)) => match op {
            Operator::Eq => v == l,
            Operator::Ne => v != l,
            // Ordering on text is locale-sensitive; not supported.
            Operator::Gt | Operator::Gte | Operator::Lt | Operator::Lte => false,
        },
        (crate::state::StateValue::Boolean(v), Literal::Boolean(l)) => match op {
            Operator::Eq => v == l,
            Operator::Ne => v != l,
            Operator::Gt | Operator::Gte | Operator::Lt | Operator::Lte => false,
        },
        // Type mismatch (including cross-type Ne): false.
        _ => false,
    }
}

/// Evaluates a condition tree against the input. Total: returns `false`
/// for missing values or type mismatches, never errors.
fn condition_value(condition: &Condition, input: &EvalInput<'_>) -> Option<bool> {
    match condition {
        Condition::All(children) => {
            let mut result = true;
            for child in children {
                if !condition_value(child, input).unwrap_or(false) {
                    result = false;
                    break;
                }
            }
            Some(result)
        }
        Condition::Any(children) => {
            let mut result = false;
            for child in children {
                if condition_value(child, input).unwrap_or(false) {
                    result = true;
                    break;
                }
            }
            Some(result)
        }
        Condition::Not(child) => Some(!condition_value(child, input).unwrap_or(false)),
        Condition::Exists { field } => match field {
            Field::EventValue => Some(input.event.is_some() || input.aggregated.is_some()),
            Field::Aggregation { .. } => Some(input.aggregated.is_some()),
            Field::State { source, kind } => Some(
                input
                    .state
                    .get(&crate::state::StateKey::new(source.clone(), kind.clone()))
                    .is_some(),
            ),
        },
        Condition::Comparison { field, op, value } => {
            let resolved = field_value(field, input)?;
            let current: crate::state::StateValue = match resolved {
                FieldResolution::Value(value) => value,
                FieldResolution::State(key) => {
                    let entry = input.state.get(&key)?;
                    entry.value.clone()
                }
            };
            Some(value_satisfies(&current, op, value))
        }
    }
}

/// Validates a condition tree at configuration time: non-empty combinator
/// lists and scalar literals only.
fn validate_condition(rule_id: &str, condition: &Condition) -> Result<(), RuleConfigError> {
    match condition {
        Condition::All(children) | Condition::Any(children) => {
            if children.is_empty() {
                return Err(RuleConfigError::EmptyConditionList(rule_id.to_string()));
            }
            for child in children {
                validate_condition(rule_id, child)?;
            }
            Ok(())
        }
        Condition::Not(child) => validate_condition(rule_id, child),
        Condition::Comparison { value, .. } => match value {
            Literal::Numeric(_) | Literal::Text(_) | Literal::Boolean(_) => Ok(()),
        },
        Condition::Exists { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Payload;
    use crate::state::{StateConfig, StateStore};

    fn store() -> StateStore {
        StateStore::new(StateConfig::default()).unwrap()
    }

    fn numeric_event(id: &str, kind: &str, ts: i64, value: f64) -> Event {
        Event::new(id, "sensor-1", kind, ts, Payload::Numeric { value }).unwrap()
    }

    fn text_event(id: &str, kind: &str, ts: i64, value: &str) -> Event {
        Event::new(
            id,
            "sensor-1",
            kind,
            ts,
            Payload::Text {
                value: value.to_string(),
            },
        )
        .unwrap()
    }

    fn text_event_on(id: &str, source: &str, kind: &str, ts: i64, value: &str) -> Event {
        Event::new(
            id,
            source,
            kind,
            ts,
            Payload::Text {
                value: value.to_string(),
            },
        )
        .unwrap()
    }

    fn summary(kind: &str, end_ms: i64, mean: f64) -> Aggregated {
        Aggregated {
            source: "sensor-1".to_string(),
            kind: kind.to_string(),
            window_start_ms: end_ms - 1_000,
            window_end_ms: end_ms,
            count: 2,
            min: mean - 1.0,
            max: mean + 1.0,
            sum: mean * 2.0,
            mean,
            last: mean + 1.0,
        }
    }

    fn rule(id: &str, action: &str, condition: Condition) -> Rule {
        Rule {
            id: id.to_string(),
            description: None,
            on: Trigger::Event {
                kind: "temperature".to_string(),
            },
            condition,
            action: action.to_string(),
            suppression: None,
        }
    }

    fn comparison(field: Field, op: Operator, value: Literal) -> Condition {
        Condition::Comparison { field, op, value }
    }

    // --- Operators ---

    #[test]
    fn numeric_operators_hold() {
        let state = store();
        let event = numeric_event("e", "temperature.reading", 1_000, 85.0);
        let input = EvalInput::from_event(&event, &state);

        let cases = [
            (Operator::Gt, 80.0, true),
            (Operator::Gt, 85.0, false),
            (Operator::Gte, 85.0, true),
            (Operator::Lt, 90.0, true),
            (Operator::Lt, 85.0, false),
            (Operator::Lte, 85.0, true),
            (Operator::Eq, 85.0, true),
            (Operator::Ne, 85.0, false),
            (Operator::Ne, 84.0, true),
        ];
        for (op, literal, expected) in cases {
            let mut engine = RuleEngine::new(vec![rule(
                "r",
                "a",
                comparison(Field::EventValue, op, Literal::Numeric(literal)),
            )])
            .unwrap();
            let fired = engine.evaluate(&input);
            assert_eq!(!fired.is_empty(), expected, "{op:?} {literal}");
        }
    }

    #[test]
    fn text_equality_operators_hold() {
        let state = store();
        let event = text_event("e", "door.state", 1_000, "open");
        let input = EvalInput::from_event(&event, &state);

        let mk = |id: &str, literal: &str| Rule {
            id: id.to_string(),
            description: None,
            on: Trigger::Event {
                kind: "door".to_string(),
            },
            condition: comparison(
                Field::EventValue,
                Operator::Eq,
                Literal::Text(literal.to_string()),
            ),
            action: "notify".to_string(),
            suppression: None,
        };

        let mut engine = RuleEngine::new(vec![mk("door-open", "open")]).unwrap();
        assert_eq!(engine.evaluate(&input).len(), 1);

        let mut engine = RuleEngine::new(vec![mk("door-closed", "closed")]).unwrap();
        assert!(engine.evaluate(&input).is_empty());
    }

    #[test]
    fn boolean_equality_operators_hold() {
        let state = store();
        let event = Event::new(
            "e",
            "sensor-1",
            "door.contact",
            1_000,
            Payload::Boolean { value: true },
        )
        .unwrap();
        let input = EvalInput::from_event(&event, &state);

        let mut engine = RuleEngine::new(vec![Rule {
            id: "contact".to_string(),
            description: None,
            on: Trigger::Event {
                kind: "door".to_string(),
            },
            condition: comparison(Field::EventValue, Operator::Eq, Literal::Boolean(true)),
            action: "notify".to_string(),
            suppression: None,
        }])
        .unwrap();
        assert_eq!(engine.evaluate(&input).len(), 1);
    }

    // --- Combinators / truth tables ---

    #[test]
    fn all_and_any_and_not_truth_tables() {
        let state = store();
        let event = numeric_event("e", "temperature.reading", 1_000, 85.0);
        let input = EvalInput::from_event(&event, &state);

        let hot = comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0));
        let cold = comparison(Field::EventValue, Operator::Lt, Literal::Numeric(10.0));

        let mut engine = RuleEngine::new(vec![
            rule(
                "all-true",
                "a",
                Condition::All(vec![hot.clone(), hot.clone()]),
            ),
            rule(
                "all-false",
                "a",
                Condition::All(vec![hot.clone(), cold.clone()]),
            ),
            rule(
                "any-true",
                "a",
                Condition::Any(vec![hot.clone(), cold.clone()]),
            ),
            rule(
                "any-false",
                "a",
                Condition::Any(vec![cold.clone(), cold.clone()]),
            ),
            rule("not-hot", "a", Condition::Not(Box::new(hot.clone()))),
            rule("not-cold", "a", Condition::Not(Box::new(cold.clone()))),
        ])
        .unwrap();

        let fired: Vec<String> = engine
            .evaluate(&input)
            .into_iter()
            .map(|r| r.rule_id)
            .collect();
        // hot=85>80 true, cold=85<10 false:
        // all-true fires, all-false does not, any-true fires, any-false
        // does not, not-hot does not (hot is true), not-cold fires.
        // Id order is deterministic.
        assert_eq!(fired, vec!["all-true", "any-true", "not-cold"]);
    }

    // --- Missing state / type mismatches ---

    #[test]
    fn missing_state_compares_false_not_error() {
        let state = store();
        let event = numeric_event("e", "temperature.reading", 1_000, 85.0);
        let input = EvalInput::from_event(&event, &state);

        let mut engine = RuleEngine::new(vec![rule(
            "needs-state",
            "a",
            comparison(
                Field::State {
                    source: "sensor-1".into(),
                    kind: "door.state".into(),
                },
                Operator::Eq,
                Literal::Text("open".into()),
            ),
        )])
        .unwrap();
        assert!(engine.evaluate(&input).is_empty());
    }

    #[test]
    fn present_state_compares_true() {
        let mut state = store();
        state.observe(&crate::processing::Output::Event(text_event_on(
            "d",
            "sensor-1",
            "door.state",
            1_000,
            "open",
        )));
        let event = numeric_event("e", "temperature.reading", 2_000, 85.0);
        let input = EvalInput::from_event(&event, &state);

        let mut engine = RuleEngine::new(vec![rule(
            "door-open-and-hot",
            "a",
            Condition::All(vec![
                comparison(
                    Field::State {
                        source: "sensor-1".into(),
                        kind: "door.state".into(),
                    },
                    Operator::Eq,
                    Literal::Text("open".into()),
                ),
                comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
            ]),
        )])
        .unwrap();
        assert_eq!(engine.evaluate(&input).len(), 1);
    }

    #[test]
    fn type_mismatch_compares_false_including_ne() {
        let state = store();
        let event = text_event("e", "door.state", 1_000, "open");
        let input = EvalInput::from_event(&event, &state);

        for op in [Operator::Gt, Operator::Eq, Operator::Ne] {
            let mut engine = RuleEngine::new(vec![rule(
                "mismatch",
                "a",
                comparison(Field::EventValue, op, Literal::Numeric(5.0)),
            )])
            .unwrap();
            assert!(
                engine.evaluate(&input).is_empty(),
                "{op:?} on text-vs-numeric must be false"
            );
        }
    }

    // --- Triggers ---

    #[test]
    fn event_trigger_matches_kind_family_not_siblings() {
        let state = store();
        let event = numeric_event("e", "temperature.reading", 1_000, 85.0);

        let mut engine = RuleEngine::new(vec![rule(
            "family",
            "a",
            comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
        )])
        .unwrap();
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_event(&event, &state))
                .len(),
            1
        );

        let other = numeric_event("e2", "temperatures.x", 1_000, 85.0);
        assert!(
            engine
                .evaluate(&EvalInput::from_event(&other, &state))
                .is_empty()
        );
    }

    #[test]
    fn event_trigger_matches_summary_kind_so_aggregation_field_is_reachable() {
        let state = store();
        let mut engine = RuleEngine::new(vec![rule(
            "hot-window",
            "a",
            comparison(
                Field::Aggregation { stat: Stat::Mean },
                Operator::Gt,
                Literal::Numeric(30.0),
            ),
        )])
        .unwrap();

        let fired = engine.evaluate(&EvalInput::from_aggregated(
            &summary("temperature.reading", 61_000, 32.5),
            &state,
        ));
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn state_trigger_fires_when_key_exists() {
        let mut state = store();
        state.observe(&crate::processing::Output::Event(text_event_on(
            "d",
            "sensor-1",
            "door.state",
            1_000,
            "open",
        )));
        let unrelated = numeric_event("e", "unrelated.kind", 2_000, 1.0);

        let mut engine = RuleEngine::new(vec![Rule {
            id: "door-state-exists".to_string(),
            description: None,
            on: Trigger::State {
                source: "sensor-1".to_string(),
                kind: "door.state".to_string(),
            },
            condition: comparison(
                Field::State {
                    source: "sensor-1".into(),
                    kind: "door.state".into(),
                },
                Operator::Eq,
                Literal::Text("open".into()),
            ),
            action: "a".to_string(),
            suppression: None,
        }])
        .unwrap();

        // State trigger is a candidate on every input and fires while the
        // key exists.
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_event(&unrelated, &state))
                .len(),
            1
        );
    }

    #[test]
    fn state_trigger_stays_silent_while_key_absent() {
        let state = store();
        let event = numeric_event("e", "unrelated.kind", 2_000, 1.0);
        let mut engine = RuleEngine::new(vec![Rule {
            id: "door-state".to_string(),
            description: None,
            on: Trigger::State {
                source: "sensor-1".to_string(),
                kind: "door.state".to_string(),
            },
            condition: Condition::Exists {
                field: Field::State {
                    source: "sensor-1".into(),
                    kind: "door.state".into(),
                },
            },
            action: "a".to_string(),
            suppression: None,
        }])
        .unwrap();
        assert!(
            engine
                .evaluate(&EvalInput::from_event(&event, &state))
                .is_empty()
        );
    }

    // --- Suppression ---

    #[test]
    fn suppression_fires_once_per_event_time_window() {
        let state = store();
        let make = |id: &str, ts: i64| numeric_event(id, "temperature.reading", ts, 85.0);
        let mk_rule = |id: &str| Rule {
            id: id.to_string(),
            description: None,
            on: Trigger::Event {
                kind: "temperature".to_string(),
            },
            condition: comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
            action: "a".to_string(),
            suppression: Some(OncePerWindow { window_ms: 1_000 }),
        };

        let mut engine = RuleEngine::new(vec![mk_rule("r")]).unwrap();
        // Window 0: first fires, second suppressed.
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_event(&make("a", 100), &state))
                .len(),
            1
        );
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_event(&make("b", 500), &state))
                .len(),
            0
        );
        assert_eq!(engine.suppressed(), 1);
        // Window 1: fires again.
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_event(&make("c", 1_100), &state))
                .len(),
            1
        );
        assert_eq!(engine.suppressed(), 1);
    }

    #[test]
    fn suppression_uses_summary_window_end_as_event_time() {
        let state = store();
        let mk_rule = || Rule {
            id: "r".to_string(),
            description: None,
            on: Trigger::Event {
                kind: "temperature".to_string(),
            },
            condition: comparison(
                Field::Aggregation { stat: Stat::Mean },
                Operator::Gt,
                Literal::Numeric(30.0),
            ),
            action: "a".to_string(),
            suppression: Some(OncePerWindow { window_ms: 60_000 }),
        };

        let mut engine = RuleEngine::new(vec![mk_rule()]).unwrap();
        let first = summary("temperature.reading", 60_000, 32.0);
        let second = summary("temperature.reading", 119_999, 33.0); // same window 1? 119999/60000 = 1
        let third = summary("temperature.reading", 120_000, 34.0); // window 2

        assert_eq!(
            engine
                .evaluate(&EvalInput::from_aggregated(&first, &state))
                .len(),
            1
        );
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_aggregated(&second, &state))
                .len(),
            0
        );
        assert_eq!(
            engine
                .evaluate(&EvalInput::from_aggregated(&third, &state))
                .len(),
            1
        );
        assert_eq!(engine.suppressed(), 1);
    }

    // --- Deterministic ordering ---

    #[test]
    fn matches_are_ordered_by_rule_id_regardless_of_definition_order() {
        let state = store();
        let event = numeric_event("e", "temperature.reading", 1_000, 85.0);
        let mut engine = RuleEngine::new(vec![
            rule(
                "zeta",
                "a",
                comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
            ),
            rule(
                "alpha",
                "a",
                comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
            ),
            rule(
                "mid",
                "a",
                comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
            ),
        ])
        .unwrap();

        let ids: Vec<String> = engine
            .evaluate(&EvalInput::from_event(&event, &state))
            .into_iter()
            .map(|r| r.rule_id)
            .collect();
        assert_eq!(ids, vec!["alpha", "mid", "zeta"]);
    }

    // --- Config errors ---

    #[test]
    fn config_errors_are_rejected_with_right_variants() {
        let base = |id: &str| {
            rule(
                id,
                "a",
                Condition::Exists {
                    field: Field::EventValue,
                },
            )
        };

        assert_eq!(
            RuleEngine::new(vec![base("")]).unwrap_err(),
            RuleConfigError::EmptyRuleId
        );
        assert_eq!(
            RuleEngine::new(vec![base("r"), base("r")]).unwrap_err(),
            RuleConfigError::DuplicateRuleId("r".to_string())
        );
        let empty_action = Rule {
            id: "r".to_string(),
            description: None,
            on: Trigger::Event {
                kind: "k".to_string(),
            },
            condition: Condition::Exists {
                field: Field::EventValue,
            },
            action: "  ".to_string(),
            suppression: None,
        };
        assert_eq!(
            RuleEngine::new(vec![empty_action]).unwrap_err(),
            RuleConfigError::EmptyActionId
        );
        assert_eq!(
            RuleEngine::new(vec![rule("r", "a", Condition::All(vec![]))]).unwrap_err(),
            RuleConfigError::EmptyConditionList("r".to_string())
        );
        let bad_window = Rule {
            id: "r".to_string(),
            description: None,
            on: Trigger::Event {
                kind: "k".to_string(),
            },
            condition: Condition::Exists {
                field: Field::EventValue,
            },
            action: "a".to_string(),
            suppression: Some(OncePerWindow { window_ms: 0 }),
        };
        assert_eq!(
            RuleEngine::new(vec![bad_window]).unwrap_err(),
            RuleConfigError::InvalidSuppressionWindow {
                rule_id: "r".to_string(),
                window_ms: 0
            }
        );
    }

    // --- Serde round-trip of definitions ---

    #[test]
    fn rule_definitions_round_trip_through_json() {
        let original = Rule {
            id: "hot-room".to_string(),
            description: Some("Temperature too hot".to_string()),
            on: Trigger::Event {
                kind: "temperature".to_string(),
            },
            condition: Condition::All(vec![
                comparison(Field::EventValue, Operator::Gt, Literal::Numeric(80.0)),
                Condition::Exists {
                    field: Field::State {
                        source: "sensor-1".into(),
                        kind: "door.state".into(),
                    },
                },
            ]),
            action: "webhook-ops".to_string(),
            suppression: Some(OncePerWindow { window_ms: 60_000 }),
        };

        let json = serde_json::to_string(&original).unwrap();
        let back: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }
}
