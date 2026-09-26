//! High-level rule DSL parser: compiles developer-friendly YAML rule syntax
//! into DELTU's typed AST (`Trigger`, `Condition`, `Field`, `Operator`, `Literal`).

use serde::{Deserialize, Serialize};

use crate::rules::definition::{
    Condition, Field, Literal, OncePerWindow, Operator, Rule, Stat, Trigger,
};

/// High-level developer-facing rule definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslRuleConfig {
    /// Unique rule identifier.
    pub id: Option<String>,
    /// Rule name (used as ID if `id` is omitted).
    pub name: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Condition expression string (e.g. `temperature > 80`, `count(http.error) > 10`).
    pub when: String,
    /// Time window for once-per-window suppression (e.g. `60s`, `1m`).
    pub within: Option<String>,
    /// Target action identifier.
    pub action: Option<String>,
}

impl DslRuleConfig {
    /// Compiles the DSL configuration into a low-level [`Rule`].
    pub fn compile(self) -> Result<Rule, String> {
        let rule_id = self
            .id
            .or(self.name)
            .ok_or_else(|| "DSL rule must specify 'id' or 'name'".to_string())?;

        let action_id = self.action.unwrap_or_else(|| format!("{rule_id}-action"));

        let (trigger, condition) = parse_when_expression(&self.when)?;

        let suppression = if let Some(within_str) = self.within {
            let window_ms = parse_duration_ms(&within_str)?;
            Some(OncePerWindow { window_ms })
        } else {
            None
        };

        Ok(Rule {
            id: rule_id,
            description: self.description,
            on: trigger,
            condition,
            action: action_id,
            suppression,
        })
    }
}

/// Parses a duration string (e.g. "60s", "30s", "1m", "500ms") into milliseconds.
pub fn parse_duration_ms(input: &str) -> Result<i64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty duration string".to_string());
    }
    if let Some(num) = s.strip_suffix("ms") {
        let ms: i64 = num
            .parse()
            .map_err(|_| format!("invalid milliseconds in {input:?}"))?;
        if ms <= 0 {
            return Err(format!("duration must be positive, got {input:?}"));
        }
        Ok(ms)
    } else if let Some(num) = s.strip_suffix('s') {
        let secs: i64 = num
            .parse()
            .map_err(|_| format!("invalid seconds in {input:?}"))?;
        if secs <= 0 {
            return Err(format!("duration must be positive, got {input:?}"));
        }
        Ok(secs * 1_000)
    } else if let Some(num) = s.strip_suffix('m') {
        let mins: i64 = num
            .parse()
            .map_err(|_| format!("invalid minutes in {input:?}"))?;
        if mins <= 0 {
            return Err(format!("duration must be positive, got {input:?}"));
        }
        Ok(mins * 60_000)
    } else if let Ok(secs) = s.parse::<i64>() {
        if secs <= 0 {
            return Err(format!("duration must be positive, got {input:?}"));
        }
        Ok(secs * 1_000)
    } else {
        Err(format!(
            "unsupported duration format {input:?} (expected e.g. '60s', '1m', '500ms')"
        ))
    }
}

/// Parses a `when` expression string into a (`Trigger`, `Condition`) pair.
pub fn parse_when_expression(expression: &str) -> Result<(Trigger, Condition), String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err("empty 'when' expression".to_string());
    }

    // Handle AND combinator (top-level)
    if let Some((left, right)) = split_top_level_combinator(trimmed, " AND ") {
        let (t1, c1) = parse_when_expression(left)?;
        let (_, c2) = parse_when_expression(right)?;
        return Ok((t1, Condition::All(vec![c1, c2])));
    }

    // Handle OR combinator (top-level)
    if let Some((left, right)) = split_top_level_combinator(trimmed, " OR ") {
        let (t1, c1) = parse_when_expression(left)?;
        let (_, c2) = parse_when_expression(right)?;
        return Ok((t1, Condition::Any(vec![c1, c2])));
    }

    // Handle NOT operator
    if let Some(rest) = trimmed.strip_prefix("NOT ") {
        let (t, c) = parse_when_expression(rest)?;
        return Ok((t, Condition::Not(Box::new(c))));
    }

    // Parse single comparison expression: e.g. "count(http.error) > 10", "temperature > 80", "door == 'open'"
    parse_single_comparison(trimmed)
}

fn split_top_level_combinator<'a>(s: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    if let Some(idx) = s.find(op) {
        let left = s[..idx].trim();
        let right = s[idx + op.len()..].trim();
        if !left.is_empty() && !right.is_empty() {
            return Some((left, right));
        }
    }
    None
}

fn parse_single_comparison(expr: &str) -> Result<(Trigger, Condition), String> {
    // Determine operator
    let ops = [
        (">=", Operator::Gte),
        ("<=", Operator::Lte),
        ("==", Operator::Eq),
        ("!=", Operator::Ne),
        ("=", Operator::Eq),
        (">", Operator::Gt),
        ("<", Operator::Lt),
    ];

    let mut found_op: Option<(&str, Operator)> = None;
    for (op_str, op_enum) in ops {
        if expr.contains(op_str) {
            found_op = Some((op_str, op_enum));
            break;
        }
    }

    let (op_str, op) = found_op.ok_or_else(|| {
        format!(
            "Invalid rule syntax at: {:?}\nExpected comparison operator (>, >=, <, <=, ==, !=)",
            expr
        )
    })?;

    let parts: Vec<&str> = expr.splitn(2, op_str).collect();
    if parts.len() != 2 {
        return Err(format!("malformed comparison expression {expr:?}"));
    }

    let lhs = parts[0].trim();
    let rhs = parts[1].trim();

    // Parse LHS: can be `count(kind)`, `mean(kind)`, `min(kind)`, `max(kind)`, `sum(kind)`, `last(kind)`, or plain `kind`
    let (trigger, field) = parse_lhs(lhs)?;
    let literal = parse_rhs(rhs)?;

    Ok((
        trigger,
        Condition::Comparison {
            field,
            op,
            value: literal,
        },
    ))
}

fn parse_lhs(lhs: &str) -> Result<(Trigger, Field), String> {
    if lhs.is_empty() {
        return Err("empty left-hand side in rule condition".to_string());
    }

    let stats = [
        ("count(", Stat::Count),
        ("mean(", Stat::Mean),
        ("min(", Stat::Min),
        ("max(", Stat::Max),
        ("sum(", Stat::Sum),
        ("last(", Stat::Last),
    ];

    for (prefix, stat) in stats {
        if lhs.starts_with(prefix) && lhs.ends_with(')') {
            let inner = lhs[prefix.len()..lhs.len() - 1].trim();
            if inner.is_empty() {
                return Err(format!("empty aggregation target in {lhs:?}"));
            }
            return Ok((
                Trigger::Event {
                    kind: inner.to_string(),
                },
                Field::Aggregation { stat },
            ));
        }
    }

    // Plain event field
    Ok((
        Trigger::Event {
            kind: lhs.to_string(),
        },
        Field::EventValue,
    ))
}

fn parse_rhs(rhs: &str) -> Result<Literal, String> {
    let s = rhs.trim();
    if s.is_empty() {
        return Err("empty right-hand value in rule condition".to_string());
    }

    // Quoted text literal: 'text' or "text"
    if (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
        || (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
    {
        return Ok(Literal::Text(s[1..s.len() - 1].to_string()));
    }

    // Boolean literal
    if s.eq_ignore_ascii_case("true") {
        return Ok(Literal::Boolean(true));
    }
    if s.eq_ignore_ascii_case("false") {
        return Ok(Literal::Boolean(false));
    }

    // Numeric literal
    if let Ok(val) = s.parse::<f64>() {
        return Ok(Literal::Numeric(val));
    }

    // Unquoted string fallback if non-numeric
    Ok(Literal::Text(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_numeric_dsl() {
        let dsl = DslRuleConfig {
            id: Some("high-temp".to_string()),
            name: None,
            description: Some("Warn when temp > 80".to_string()),
            when: "temperature > 80".to_string(),
            within: Some("60s".to_string()),
            action: Some("log-ops".to_string()),
        };

        let rule = dsl.compile().unwrap();
        assert_eq!(rule.id, "high-temp");
        assert_eq!(rule.action, "log-ops");
        assert_eq!(
            rule.on,
            Trigger::Event {
                kind: "temperature".to_string()
            }
        );
        assert_eq!(rule.suppression, Some(OncePerWindow { window_ms: 60000 }));

        if let Condition::Comparison { field, op, value } = rule.condition {
            assert_eq!(field, Field::EventValue);
            assert_eq!(op, Operator::Gt);
            assert_eq!(value, Literal::Numeric(80.0));
        } else {
            panic!("expected comparison condition");
        }
    }

    #[test]
    fn parse_aggregation_count_dsl() {
        let dsl = DslRuleConfig {
            id: None,
            name: Some("error-storm".to_string()),
            description: None,
            when: "count(http.error) > 10".to_string(),
            within: Some("30s".to_string()),
            action: Some("alert-webhook".to_string()),
        };

        let rule = dsl.compile().unwrap();
        assert_eq!(rule.id, "error-storm");
        assert_eq!(
            rule.on,
            Trigger::Event {
                kind: "http.error".to_string()
            }
        );

        if let Condition::Comparison { field, op, value } = rule.condition {
            assert_eq!(field, Field::Aggregation { stat: Stat::Count });
            assert_eq!(op, Operator::Gt);
            assert_eq!(value, Literal::Numeric(10.0));
        } else {
            panic!("expected aggregation comparison");
        }
    }

    #[test]
    fn parse_text_literal_dsl() {
        let dsl = DslRuleConfig {
            id: Some("door-alert".to_string()),
            name: None,
            description: None,
            when: "door == 'open'".to_string(),
            within: None,
            action: Some("log-ops".to_string()),
        };

        let rule = dsl.compile().unwrap();
        if let Condition::Comparison { field, op, value } = rule.condition {
            assert_eq!(field, Field::EventValue);
            assert_eq!(op, Operator::Eq);
            assert_eq!(value, Literal::Text("open".to_string()));
        } else {
            panic!("expected text comparison");
        }
    }

    #[test]
    fn parse_combinator_dsl() {
        let (t, c) = parse_when_expression("temperature > 80 AND count(cpu.spike) >= 5").unwrap();
        assert_eq!(
            t,
            Trigger::Event {
                kind: "temperature".to_string()
            }
        );
        if let Condition::All(children) = c {
            assert_eq!(children.len(), 2);
        } else {
            panic!("expected All condition");
        }
    }

    #[test]
    fn duration_parser_handles_units() {
        assert_eq!(parse_duration_ms("500ms").unwrap(), 500);
        assert_eq!(parse_duration_ms("30s").unwrap(), 30000);
        assert_eq!(parse_duration_ms("2m").unwrap(), 120000);
        assert_eq!(parse_duration_ms("10").unwrap(), 10000);
        assert!(parse_duration_ms("invalid").is_err());
    }
}
