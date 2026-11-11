//! Mathematical Vector Embedding and Decision Engine.
//!
//! Uses high-dimensional feature hashing, subword n-grams, semantic root expansions,
//! cosine geometric similarity, and calibrated sigmoid/softmax probability distributions.

use crate::decision::schema::{ChoiceOptionScore, DecisionOutcome, DecisionSchema};

const VECTOR_DIM: usize = 2048;

/// Fast, scalable mathematical vector representation of text.
#[derive(Debug, Clone)]
pub struct TextVector {
    weights: Vec<f32>,
}

impl TextVector {
    /// Projects text into a normalized D-dimensional feature space.
    pub fn embed(text: &str) -> Self {
        let mut weights = vec![0.0f32; VECTOR_DIM];
        let cleaned = text.to_lowercase();
        let words: Vec<&str> = cleaned
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();

        if words.is_empty() {
            return Self { weights };
        }

        for word in &words {
            // 1. Primary token hash
            let h1 = hash_fnv(word.as_bytes()) % (VECTOR_DIM as u64);
            let sign = if (h1 & 1) == 0 { 1.0f32 } else { -1.0f32 };
            weights[h1 as usize] += sign * 2.0;

            // 2. Semantic concept expansions (built-in fast root associations)
            if let Some(synonyms) = get_semantic_associations(word) {
                for syn in synonyms {
                    let hs = hash_fnv(syn.as_bytes()) % (VECTOR_DIM as u64);
                    let s_sign = if (hs & 1) == 0 { 1.0f32 } else { -1.0f32 };
                    weights[hs as usize] += s_sign * 1.2;
                }
            }

            // 3. Subword character n-grams (3-grams and 4-grams for morphological overlap)
            let chars: Vec<char> = word.chars().collect();
            if chars.len() >= 3 {
                for window in chars.windows(3) {
                    let s: String = window.iter().collect();
                    let h = hash_fnv(s.as_bytes()) % (VECTOR_DIM as u64);
                    let s_sign = if (h & 1) == 0 { 1.0f32 } else { -1.0f32 };
                    weights[h as usize] += s_sign * 0.6;
                }
            }
            if chars.len() >= 4 {
                for window in chars.windows(4) {
                    let s: String = window.iter().collect();
                    let h = hash_fnv(s.as_bytes()) % (VECTOR_DIM as u64);
                    let s_sign = if (h & 1) == 0 { 1.0f32 } else { -1.0f32 };
                    weights[h as usize] += s_sign * 0.4;
                }
            }
        }

        // L2 vector normalization (unit sphere)
        let norm_sq: f32 = weights.iter().map(|v| v * v).sum();
        if norm_sq > 0.0 {
            let inv_norm = 1.0 / norm_sq.sqrt();
            for v in &mut weights {
                *v *= inv_norm;
            }
        }

        Self { weights }
    }

    /// Computes geometric cosine similarity between two normalized vectors: cos(theta) = u . v.
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        let dot: f32 = self
            .weights
            .iter()
            .zip(other.weights.iter())
            .map(|(a, b)| a * b)
            .sum();
        dot.clamp(-1.0, 1.0)
    }
}

/// Evaluates a typed decision schema over the input text mathematically.
pub fn evaluate_decision(input: &str, schema: &DecisionSchema) -> Result<DecisionOutcome, String> {
    let input_vec = TextVector::embed(input);

    match schema {
        DecisionSchema::Choice {
            options,
            temperature,
        } => {
            if options.is_empty() {
                return Err("Choice schema requires at least one option".to_string());
            }

            let mut raw_scores = Vec::with_capacity(options.len());
            for opt in options {
                let opt_vec = TextVector::embed(opt);
                let sim = input_vec.cosine_similarity(&opt_vec);
                raw_scores.push((opt.clone(), sim));
            }

            // Temperature-scaled Softmax
            let temp = if *temperature <= 0.0 {
                12.0
            } else {
                *temperature
            };
            let max_scaled = raw_scores
                .iter()
                .map(|(_, s)| s * temp)
                .fold(f32::NEG_INFINITY, f32::max);

            let exps: Vec<f32> = raw_scores
                .iter()
                .map(|(_, s)| ((s * temp) - max_scaled).exp())
                .collect();
            let sum_exps: f32 = exps.iter().sum();

            let mut rankings: Vec<ChoiceOptionScore> = raw_scores
                .into_iter()
                .zip(exps)
                .map(|((opt, sim), exp)| ChoiceOptionScore {
                    option: opt,
                    probability: (exp / sum_exps.max(1e-9)).clamp(0.0, 1.0),
                    raw_similarity: sim,
                })
                .collect();

            // Sort descending by probability
            rankings.sort_by(|a, b| {
                b.probability
                    .partial_cmp(&a.probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let top = rankings
                .first()
                .cloned()
                .unwrap_or_else(|| ChoiceOptionScore {
                    option: String::new(),
                    probability: 1.0,
                    raw_similarity: 1.0,
                });

            Ok(DecisionOutcome::Choice {
                selected: top.option,
                confidence: top.probability,
                rankings,
            })
        }

        DecisionSchema::Score { criteria, min, max } => {
            let criteria_vec = TextVector::embed(criteria);
            let sim = input_vec.cosine_similarity(&criteria_vec);

            // Calibrated logistic sigmoid mapping similarity to [0.0, 1.0]
            let normalized = (1.0 / (1.0 + (-12.0 * (sim - 0.08)).exp()) as f64).clamp(0.0, 1.0);
            let score = min + normalized * (max - min);

            Ok(DecisionOutcome::Score {
                criteria: criteria.clone(),
                score,
                normalized,
                raw_similarity: sim,
            })
        }

        DecisionSchema::YesNo {
            condition,
            threshold,
        } => {
            let cond_vec = TextVector::embed(condition);
            let sim = input_vec.cosine_similarity(&cond_vec);

            // Calibrated probability curve: similarity > 0.10 indicates active semantic match
            let prob = (1.0 / (1.0 + (-12.0 * (sim - 0.10)).exp())).clamp(0.0, 1.0);
            let result = prob >= *threshold;

            Ok(DecisionOutcome::YesNo {
                condition: condition.clone(),
                result,
                probability: prob,
                threshold: *threshold,
            })
        }
    }
}

/// Fast FNV-1a hash function.
fn hash_fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

/// Fast built-in semantic concept dictionary for root domain associations.
fn get_semantic_associations(word: &str) -> Option<&'static [&'static str]> {
    match word {
        // Financial & Billing
        "credit" | "card" | "charge" | "charged" | "bill" | "billing" | "billed" | "invoice"
        | "payment" | "pay" | "fee" | "refund" | "price" | "cost" | "subscription" | "renew"
        | "renewal" | "overbill" | "overbilled" => {
            Some(&["billing", "payment", "finance", "money"])
        }

        // Technical Support & Bugs
        "bug" | "bugs" | "crash" | "crashed" | "error" | "500" | "fail" | "failed" | "failure"
        | "broken" | "down" | "outage" | "timeout" | "exception" | "deadlock" | "panic"
        | "glitch" | "memory" | "database" | "server" | "cluster" | "infra" | "infrastructure" => {
            Some(&[
                "technical",
                "support",
                "engineering",
                "bug",
                "issue",
                "infrastructure",
            ])
        }

        // Authentication & Security
        "password" | "passcode" | "login" | "signin" | "auth" | "authentication" | "2fa"
        | "mfa" | "locked" | "token" | "reset" => {
            Some(&["security", "password", "authentication", "account"])
        }

        // Sales & Inquiries
        "sales" | "pricing" | "enterprise" | "quote" | "demo" | "inquiry" | "purchase" | "buy"
        | "license" | "upgrade" => Some(&["sales", "enterprise", "inquiry", "commercial"]),

        // Severity & Urgency
        "critical" | "urgent" | "emergency" | "sev1" | "catastrophe" | "immediate" | "blocker" => {
            Some(&["severity", "critical", "urgent", "priority", "emergency"])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_have_max_cosine_similarity() {
        let v1 = TextVector::embed("urgent server down");
        let v2 = TextVector::embed("urgent server down");
        let sim = v1.cosine_similarity(&v2);
        assert!((sim - 1.0).abs() < 1e-4);
    }

    #[test]
    fn unrelated_texts_have_low_similarity() {
        let v1 = TextVector::embed("apple banana fruit smoothie");
        let v2 = TextVector::embed("database transaction deadlock kernel");
        let sim = v1.cosine_similarity(&v2);
        assert!(sim < 0.2, "similarity was {sim}");
    }

    #[test]
    fn choice_decision_picks_best_semantic_candidate() {
        let schema = DecisionSchema::Choice {
            options: vec![
                "billing and payment".to_string(),
                "technical support and bugs".to_string(),
                "sales and enterprise inquiry".to_string(),
            ],
            temperature: 10.0,
        };

        let outcome =
            evaluate_decision("My credit card was charged twice for the renewal", &schema).unwrap();
        let DecisionOutcome::Choice {
            selected,
            confidence,
            rankings,
        } = outcome
        else {
            panic!("expected Choice outcome");
        };

        assert_eq!(selected, "billing and payment");
        assert!(
            confidence > 0.5,
            "expected high confidence, got {confidence}"
        );
        assert_eq!(rankings.len(), 3);
        assert_eq!(rankings[0].option, "billing and payment");
    }

    #[test]
    fn score_decision_rates_relevance() {
        let schema = DecisionSchema::Score {
            criteria: "critical production emergency".to_string(),
            min: 0.0,
            max: 100.0,
        };

        let high =
            evaluate_decision("Production cluster is down and returning 500s", &schema).unwrap();
        let low = evaluate_decision("What font does the website use?", &schema).unwrap();

        let DecisionOutcome::Score {
            score: high_score, ..
        } = high
        else {
            panic!()
        };
        let DecisionOutcome::Score {
            score: low_score, ..
        } = low
        else {
            panic!()
        };

        assert!(
            high_score > low_score,
            "high: {high_score}, low: {low_score}"
        );
        assert!(high_score >= 50.0);
    }

    #[test]
    fn yes_no_decision_calibrates_binary_outcome() {
        let schema = DecisionSchema::YesNo {
            condition: "password reset request".to_string(),
            threshold: 0.5,
        };

        let res_yes = evaluate_decision("I forgot my password and cannot log in", &schema).unwrap();
        let res_no = evaluate_decision("How much does the enterprise tier cost?", &schema).unwrap();

        let DecisionOutcome::YesNo {
            result: r1,
            probability: p1,
            ..
        } = res_yes
        else {
            panic!()
        };
        let DecisionOutcome::YesNo {
            result: r2,
            probability: p2,
            ..
        } = res_no
        else {
            panic!()
        };

        assert!(r1, "expected true, got false (prob {p1})");
        assert!(!r2, "expected false, got true (prob {p2})");
    }
}
