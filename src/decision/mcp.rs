//! Model Context Protocol (MCP) Server for the Deltu Decision Engine.
//!
//! Exposes non-autoregressive System 1 decision tools (`deltu_choice`,
//! `deltu_score`, `deltu_yesno`, `deltu_evaluate`) over standard I/O (stdio).
//! Any MCP client (Claude Code, Cursor, Zed, Antigravity) can connect and
//! invoke sub-millisecond mathematical decisions.

use serde_json::{Value as JsonValue, json};
use std::io::{BufRead, Write};

use crate::decision::schema::{DecisionOutcome, DecisionSchema};
use crate::decision::vector::evaluate_decision;

/// Handles a single incoming JSON-RPC string message and returns the response if required.
pub fn handle_mcp_message(raw_msg: &str) -> Option<String> {
    let msg: JsonValue = match serde_json::from_str(raw_msg) {
        Ok(v) => v,
        Err(_) => {
            let err = json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32700,
                    "message": "Parse error: invalid JSON"
                }
            });
            return Some(err.to_string());
        }
    };

    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "initialize" => {
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "deltu",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            });
            Some(res.to_string())
        }
        "notifications/initialized" => None,
        "ping" => {
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            });
            Some(res.to_string())
        }
        "tools/list" => {
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "deltu_choice",
                            "description": "Sub-millisecond choice selection: picks the best candidate from a list of options with calibrated probability and cosine similarity.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "input": {
                                        "type": "string",
                                        "description": "The input text or query to categorize"
                                    },
                                    "options": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "List of candidate options to select from"
                                    },
                                    "temperature": {
                                        "type": "number",
                                        "description": "Optional temperature scaling parameter (default: 10.0)"
                                    }
                                },
                                "required": ["input", "options"]
                            }
                        },
                        {
                            "name": "deltu_score",
                            "description": "Sub-millisecond semantic scoring: rates an input text against a criteria or concept on a continuous scale.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "input": {
                                        "type": "string",
                                        "description": "The input text to evaluate"
                                    },
                                    "criteria": {
                                        "type": "string",
                                        "description": "Target criteria or concept (e.g. 'urgent incident', 'positive sentiment')"
                                    },
                                    "min": {
                                        "type": "number",
                                        "description": "Minimum scale bound (default: 0.0)"
                                    },
                                    "max": {
                                        "type": "number",
                                        "description": "Maximum scale bound (default: 1.0)"
                                    }
                                },
                                "required": ["input", "criteria"]
                            }
                        },
                        {
                            "name": "deltu_yesno",
                            "description": "Sub-millisecond binary gate: verifies whether an input satisfies a condition with calibrated probability.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "input": {
                                        "type": "string",
                                        "description": "The input text to verify"
                                    },
                                    "condition": {
                                        "type": "string",
                                        "description": "Condition statement to check (e.g. 'requires supervisor approval')"
                                    },
                                    "threshold": {
                                        "type": "number",
                                        "description": "Decision threshold in [0.0, 1.0] (default: 0.5)"
                                    }
                                },
                                "required": ["input", "condition"]
                            }
                        },
                        {
                            "name": "deltu_evaluate",
                            "description": "Evaluates a full typed decision schema (Choice, Score, or YesNo) over input text.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "input": {
                                        "type": "string",
                                        "description": "The input text to evaluate"
                                    },
                                    "schema": {
                                        "type": "object",
                                        "description": "The typed decision schema"
                                    }
                                },
                                "required": ["input", "schema"]
                            }
                        }
                    ]
                }
            });
            Some(res.to_string())
        }
        "tools/call" => {
            let params = msg.get("params");
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(JsonValue::Null);

            let outcome_result: Result<DecisionOutcome, String> = match tool_name {
                "deltu_choice" => {
                    let input = args.get("input").and_then(|i| i.as_str()).unwrap_or("");
                    let options: Vec<String> = args
                        .get("options")
                        .and_then(|o| o.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let temperature = args
                        .get("temperature")
                        .and_then(|t| t.as_f64())
                        .map(|t| t as f32)
                        .unwrap_or(10.0);

                    evaluate_decision(
                        input,
                        &DecisionSchema::Choice {
                            options,
                            temperature,
                        },
                    )
                }
                "deltu_score" => {
                    let input = args.get("input").and_then(|i| i.as_str()).unwrap_or("");
                    let criteria = args
                        .get("criteria")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    let min = args.get("min").and_then(|m| m.as_f64()).unwrap_or(0.0);
                    let max = args.get("max").and_then(|m| m.as_f64()).unwrap_or(1.0);

                    evaluate_decision(input, &DecisionSchema::Score { criteria, min, max })
                }
                "deltu_yesno" => {
                    let input = args.get("input").and_then(|i| i.as_str()).unwrap_or("");
                    let condition = args
                        .get("condition")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    let threshold = args
                        .get("threshold")
                        .and_then(|t| t.as_f64())
                        .map(|t| t as f32)
                        .unwrap_or(0.5);

                    evaluate_decision(
                        input,
                        &DecisionSchema::YesNo {
                            condition,
                            threshold,
                        },
                    )
                }
                "deltu_evaluate" => {
                    let input = args.get("input").and_then(|i| i.as_str()).unwrap_or("");
                    match serde_json::from_value::<DecisionSchema>(
                        args.get("schema").cloned().unwrap_or(JsonValue::Null),
                    ) {
                        Ok(schema) => evaluate_decision(input, &schema),
                        Err(err) => Err(format!("Invalid decision schema: {err}")),
                    }
                }
                unknown => Err(format!("Unknown tool: {unknown}")),
            };

            let res = match outcome_result {
                Ok(outcome) => {
                    let outcome_str =
                        serde_json::to_string_pretty(&outcome).unwrap_or_else(|_| "{}".to_string());
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": outcome_str
                                }
                            ],
                            "isError": false
                        }
                    })
                }
                Err(err) => {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": err
                                }
                            ],
                            "isError": true
                        }
                    })
                }
            };

            Some(res.to_string())
        }
        _ => {
            if id.is_some() {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {method}")
                    }
                });
                Some(err.to_string())
            } else {
                None
            }
        }
    }
}

/// Runs the MCP stdio event loop.
pub fn run_mcp_server() -> i32 {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        match line {
            Ok(content) => {
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(resp) = handle_mcp_message(trimmed) {
                    if let Err(e) = writeln!(stdout, "{resp}") {
                        eprintln!("deltu: mcp stdout write failed: {e}");
                        return 1;
                    }
                    let _ = stdout.flush();
                }
            }
            Err(e) => {
                eprintln!("deltu: mcp stdin read failed: {e}");
                return 1;
            }
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_initialize() {
        let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = handle_mcp_message(req).expect("must return response");
        let parsed: JsonValue = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["result"]["serverInfo"]["name"], "deltu");
    }

    #[test]
    fn test_mcp_tools_list() {
        let req = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let resp = handle_mcp_message(req).expect("must return response");
        let parsed: JsonValue = serde_json::from_str(&resp).unwrap();
        let tools = parsed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 4);
    }

    #[test]
    fn test_mcp_tool_call_choice() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "deltu_choice",
                "arguments": {
                    "input": "I have an issue with my monthly payment",
                    "options": ["billing", "hardware", "general"]
                }
            }
        })
        .to_string();

        let resp = handle_mcp_message(&req).expect("must return response");
        let parsed: JsonValue = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["id"], 3);
        assert_eq!(parsed["result"]["isError"], false);
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("billing"));
    }

    #[test]
    fn test_mcp_tool_call_score() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "deltu_score",
                "arguments": {
                    "input": "Critical crash in production core",
                    "criteria": "critical failure emergency"
                }
            }
        })
        .to_string();

        let resp = handle_mcp_message(&req).expect("must return response");
        let parsed: JsonValue = serde_json::from_str(&resp).unwrap();
        assert_eq!(parsed["id"], 4);
        assert_eq!(parsed["result"]["isError"], false);
    }
}
