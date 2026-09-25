//! Criterion benchmark for the Deltu rule engine
//! (`context/specs/05-rules.md`): evaluate 100 mixed-condition rules
//! against one input. Synthetic data only — no I/O.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use deltu::{
    Condition, EvalInput, Event, Field, Literal, Operator, Rule, RuleEngine, Stat, StateConfig,
    StateStore, Trigger,
};

/// Builds a rule with a mixed condition tree; panics on invalid arguments
/// because a benchmark with broken fixtures measures nothing.
fn mixed_rule(id: &str, index: usize) -> Rule {
    Rule {
        id: id.to_string(),
        description: None,
        on: Trigger::Event {
            kind: "temperature".to_string(),
        },
        condition: Condition::All(vec![
            Condition::Comparison {
                field: Field::EventValue,
                op: Operator::Gt,
                value: Literal::Numeric(index as f64),
            },
            Condition::Any(vec![
                Condition::Comparison {
                    field: Field::Aggregation { stat: Stat::Mean },
                    op: Operator::Lte,
                    value: Literal::Numeric(100.0),
                },
                Condition::Exists {
                    field: Field::State {
                        source: format!("source-{index}"),
                        kind: "bench.state".to_string(),
                    },
                },
            ]),
        ]),
        action: format!("action-{index}"),
        suppression: None,
    }
}

fn bench_evaluate_100_rules(c: &mut Criterion) {
    let rules: Vec<Rule> = (0..100)
        .map(|i| mixed_rule(&format!("rule-{i:03}"), i))
        .collect();
    let mut engine = RuleEngine::new(rules).expect("rule set must be valid");
    let state = StateStore::new(StateConfig::default()).expect("config must be valid");
    let event = Event::new(
        "bench-event",
        "bench/source",
        "temperature.reading",
        1_000,
        deltu::Payload::Numeric { value: 50.0 },
    )
    .expect("fixture must be valid");
    let input = EvalInput::from_event(&event, &state);

    c.bench_function("rules_evaluate_100", |b| {
        b.iter(|| {
            let requests = engine.evaluate(black_box(&input));
            black_box(requests.len());
        })
    });
}

criterion_group!(benches, bench_evaluate_100_rules);
criterion_main!(benches);
