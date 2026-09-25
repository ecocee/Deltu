//! The HTTP webhook action (spec 06's "executable from Unit 07" transport):
//! posts the rule snapshot as one JSON document to a configured URL.
//!
//! Failure isolation (invariant 8) shapes everything here: a fixed
//! per-request timeout, no retries, no queuing, no background buffering —
//! the executor is synchronous and bounded by construction. A refused
//! connection, a slow server, or a 500 all become counted, human-readable
//! `ActionError`s collected by the dispatcher; the engine keeps running.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::actions::{ActionError, ActionExecutor, ActionRequest, ActionSummary};

/// Default per-request timeout for webhook actions.
pub const DEFAULT_TIMEOUT_MS: u64 = 3_000;
/// Upper bound for the configurable timeout (keeps a misconfiguration
/// from parking worker threads indefinitely).
pub const MAX_TIMEOUT_MS: u64 = 30_000;
/// Cap on configured header count and size (bounded configuration).
const MAX_HEADERS: usize = 16;
const MAX_HEADER_LEN: usize = 512;

/// The shared blocking HTTP client handle.
///
/// reqwest's blocking client runs its own internal tokio thread and its
/// `wait::timeout` helper builds a throwaway current-thread runtime when
/// it detects it is already inside one. That helper **panics when it
/// drops that runtime from a tokio worker** (tokio forbids blocking
/// inside runtime context). Deltu's dispatcher executes actions directly
/// on tokio workers, so webhook executions must run on the **blocking
/// pool** — the runtime wiring wraps dispatch in
/// `tokio::task::spawn_blocking` (see `serve`), and this module also
/// spawns each request's send via a helper thread guard where needed.
/// One shared client keeps it bounded: one internal runtime for the whole
/// process, per-request timeouts, capped pool.
static WEBHOOK_CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();

/// Returns the shared client, building it on a plain thread the first
/// time (never inside a tokio runtime context).
fn shared_client(timeout_ms: u64) -> Result<&'static reqwest::blocking::Client, String> {
    if let Some(client) = WEBHOOK_CLIENT.get() {
        return Ok(client);
    }
    let built = std::thread::spawn(move || {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .connect_timeout(Duration::from_millis(timeout_ms))
            // Bounded redirect behavior: webhooks should not chase loops.
            .redirect(reqwest::redirect::Policy::limited(3))
            // No pooling growth: a small idle pool with a short lifetime
            // keeps the executor stateless and bounded.
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| format!("failed to build webhook HTTP client: {error}"))
    })
    .join()
    .map_err(|_| "webhook HTTP client builder panicked".to_string())??;
    // First builder wins; identical configuration makes races harmless.
    Ok(WEBHOOK_CLIENT.get_or_init(|| built))
}

/// A single configured header (name/value pair; `value` may contain any
/// string — no secret handling is attempted here, config is trusted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookHeader {
    /// Header name, e.g. `authorization`.
    pub name: String,
    /// Header value.
    pub value: String,
}

/// HTTP method for a webhook action. Closed set: webhooks are typically
/// POST; PUT/PATCH exist for state-update style receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WebhookMethod {
    /// POST (default).
    #[default]
    Post,
    /// PUT.
    Put,
    /// PATCH.
    Patch,
}

impl WebhookMethod {
    fn as_str(self) -> &'static str {
        match self {
            WebhookMethod::Post => "POST",
            WebhookMethod::Put => "PUT",
            WebhookMethod::Patch => "PATCH",
        }
    }
}

/// Validated configuration for one webhook action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Absolute `http://` or `https://` URL to deliver to.
    pub url: String,
    /// HTTP method. Default POST.
    #[serde(default)]
    pub method: WebhookMethod,
    /// Extra headers sent with the request.
    #[serde(default)]
    pub headers: Vec<WebhookHeader>,
    /// Per-request timeout in milliseconds. Default 3000, max 30000.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

/// Configuration-time validation. `check`/config load call this through
/// the dispatcher's constructor, so bad webhooks fail before deployment.
pub fn validate_config(config: &WebhookConfig) -> Result<(), String> {
    let url = reqwest::Url::parse(&config.url)
        .map_err(|error| format!("webhook url {:?} is not a valid URL: {error}", config.url))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "webhook url must use http:// or https://, got scheme {other:?} in {:?}",
                config.url
            ));
        }
    }
    if url.host_str().is_none() {
        return Err(format!("webhook url {:?} has no host", config.url));
    }
    if !(1..=MAX_TIMEOUT_MS).contains(&config.timeout_ms) {
        return Err(format!(
            "webhook timeout_ms must be 1..={MAX_TIMEOUT_MS}, got {}",
            config.timeout_ms
        ));
    }
    if config.headers.len() > MAX_HEADERS {
        return Err(format!(
            "webhook headers must be at most {MAX_HEADERS} entries, got {}",
            config.headers.len()
        ));
    }
    for header in &config.headers {
        if header.name.trim().is_empty() {
            return Err("webhook header name must not be empty".to_string());
        }
        if header.name.len() > MAX_HEADER_LEN || header.value.len() > MAX_HEADER_LEN {
            return Err(format!(
                "webhook header name/value must be at most {MAX_HEADER_LEN} bytes"
            ));
        }
        if header
            .name
            .contains(|c: char| c.is_whitespace() || c == ':')
        {
            return Err(format!(
                "webhook header name {:?} must not contain whitespace or ':'",
                header.name
            ));
        }
    }
    Ok(())
}

/// The webhook executor: one blocking HTTP request per execution on the
/// worker's thread (processing is synchronous; the request timeout bounds
/// the pause — this is the documented cost of a network action).
pub struct WebhookExecutor {
    config: WebhookConfig,
}

impl std::fmt::Debug for WebhookExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookExecutor")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl WebhookExecutor {
    /// Builds an executor from validated configuration. The shared
    /// process-wide HTTP client is constructed here (once) so webhook
    /// misconfiguration still fails at startup, never mid-run.
    pub fn new(config: WebhookConfig) -> Result<Self, String> {
        validate_config(&config)?;
        shared_client(config.timeout_ms)?;
        Ok(Self { config })
    }

    /// Builds the JSON body: the rule snapshot (rule id, action id,
    /// payload) plus delivery metadata — the meaningful, compressed
    /// context the rules layer already produced.
    pub fn build_body(request: &ActionRequest, now_ms: i64) -> JsonValue {
        serde_json::json!({
            "rule_id": request.rule_id,
            "action": request.action,
            "payload": request.payload,
            "ts_ms": now_ms,
        })
    }
}

impl ActionExecutor for WebhookExecutor {
    fn execute(
        &mut self,
        request: &ActionRequest,
        now_ms: i64,
    ) -> Result<ActionSummary, ActionError> {
        let body = Self::build_body(request, now_ms);
        let url = reqwest::Url::parse(&self.config.url).map_err(|error| {
            ActionError::ExecutionFailed {
                action_id: request.action.clone(),
                reason: format!("webhook url became invalid: {error}"),
            }
        })?;

        let client = shared_client(self.config.timeout_ms).map_err(|reason| {
            ActionError::ExecutionFailed {
                action_id: request.action.clone(),
                reason,
            }
        })?;
        let method = reqwest::Method::from_bytes(self.config.method.as_str().as_bytes())
            .expect("validated closed method set");
        let mut builder = client
            .request(method, url)
            .header("content-type", "application/json")
            .header("user-agent", concat!("deltu/", env!("CARGO_PKG_VERSION")))
            .json(&body);
        for header in &self.config.headers {
            builder = builder.header(header.name.as_str(), header.value.as_str());
        }

        let response = builder.send().map_err(|error| {
            let reason = if error.is_timeout() {
                format!(
                    "webhook to {:?} timed out after {} ms",
                    self.config.url, self.config.timeout_ms
                )
            } else if error.is_connect() {
                format!(
                    "webhook to {:?} could not connect: {error}",
                    self.config.url
                )
            } else {
                format!("webhook to {:?} failed: {error}", self.config.url)
            };
            ActionError::ExecutionFailed {
                action_id: request.action.clone(),
                reason,
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ActionError::ExecutionFailed {
                action_id: request.action.clone(),
                reason: format!(
                    "webhook to {:?} returned HTTP {} {}",
                    self.config.url,
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                ),
            });
        }

        Ok(ActionSummary {
            detail: format!(
                "webhook delivered to {} ({})",
                self.config.url,
                status.as_u16()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn request() -> ActionRequest {
        ActionRequest {
            rule_id: "hot-room".to_string(),
            action: "notify".to_string(),
            payload: json!({ "mean": 85.5 }),
        }
    }

    /// What the test receiver captured: request line, headers, body.
    type CapturedRequest = (String, Vec<(String, String)>, String);

    fn executor(url: &str) -> WebhookExecutor {
        WebhookExecutor::new(WebhookConfig {
            url: url.to_string(),
            method: WebhookMethod::Post,
            headers: vec![WebhookHeader {
                name: "x-deltu-test".to_string(),
                value: "yes".to_string(),
            }],
            timeout_ms: 1_000,
        })
        .unwrap()
    }

    /// One-shot local HTTP receiver returning `status`; the captured
    /// request (method, headers, body) is returned through the channel.
    fn spawn_receiver(status: u16) -> (String, std::sync::mpsc::Receiver<CapturedRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            buffer.extend_from_slice(&chunk[..n]);
                            let text = String::from_utf8_lossy(&buffer).to_string();
                            if text.contains("\r\n\r\n") {
                                let headers_end = text.find("\r\n\r\n").unwrap() + 4;
                                if let Some(length_line) = text
                                    .lines()
                                    .find(|l| l.to_lowercase().starts_with("content-length:"))
                                {
                                    let length: usize = length_line
                                        .split(':')
                                        .nth(1)
                                        .and_then(|v| v.trim().parse().ok())
                                        .unwrap_or(0);
                                    if buffer.len() >= headers_end + length {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                let raw = String::from_utf8_lossy(&buffer).to_string();
                let mut lines = raw.lines();
                let request_line = lines.next().unwrap_or_default().to_string();
                let headers: Vec<(String, String)> = lines
                    .by_ref()
                    .take_while(|l| !l.is_empty())
                    .filter_map(|l| {
                        l.split_once(':')
                            .map(|(n, v)| (n.trim().to_lowercase(), v.trim().to_string()))
                    })
                    .collect();
                let body_start = raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(raw.len());
                let body = raw[body_start..].to_string();
                let _ = tx.send((request_line, headers, body));
                let status_text = match status {
                    200 => "OK",
                    500 => "Internal Server Error",
                    _ => "Status",
                };
                let response = format!(
                    "HTTP/1.1 {status} {status_text}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (address, rx)
    }

    #[test]
    fn config_validation_rejects_bad_urls_and_values() {
        let base = |url: &str| WebhookConfig {
            url: url.to_string(),
            method: WebhookMethod::Post,
            headers: vec![],
            timeout_ms: 1_000,
        };
        assert!(validate_config(&base("http://example.com/hook")).is_ok());
        assert!(validate_config(&base("https://example.com/hook")).is_ok());
        assert!(WebhookExecutor::new(base("ftp://example.com")).is_err());
        assert!(WebhookExecutor::new(base("not a url")).is_err());
        assert!(WebhookExecutor::new(base("http://")).is_err());

        let zero_timeout = WebhookConfig {
            timeout_ms: 0,
            ..base("http://example.com/hook")
        };
        assert!(validate_config(&zero_timeout).is_err());
        let huge_timeout = WebhookConfig {
            timeout_ms: MAX_TIMEOUT_MS + 1,
            ..base("http://example.com/hook")
        };
        assert!(validate_config(&huge_timeout).is_err());
    }

    #[test]
    fn build_body_contains_rule_action_payload_timestamp() {
        let body = WebhookExecutor::build_body(&request(), 1_769_412_000_123);
        assert_eq!(body["rule_id"], "hot-room");
        assert_eq!(body["action"], "notify");
        assert_eq!(body["payload"]["mean"], 85.5);
        assert_eq!(body["ts_ms"], 1_769_412_000_123i64);
    }

    #[test]
    fn successful_delivery_reaches_receiver_with_headers() {
        let (url, rx) = spawn_receiver(200);
        let mut executor = executor(&url);
        let summary = executor.execute(&request(), 1_769_412_000_123).unwrap();
        assert!(summary.detail.contains("200"));

        let (request_line, headers, body) = rx.recv().unwrap();
        assert!(request_line.starts_with("POST "));
        let parsed: JsonValue = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["rule_id"], "hot-room");
        assert!(
            headers
                .iter()
                .any(|(n, v)| n == "x-deltu-test" && v == "yes")
        );
        assert!(
            headers
                .iter()
                .any(|(n, v)| n == "content-type" && v.starts_with("application/json"))
        );
    }

    #[test]
    fn non_2xx_response_is_a_counted_failure_not_a_panic() {
        let (url, _rx) = spawn_receiver(500);
        let mut executor = executor(&url);
        let error = executor.execute(&request(), 1).unwrap_err();
        match error {
            ActionError::ExecutionFailed { reason, .. } => {
                assert!(reason.contains("500"), "reason: {reason}");
            }
            other => panic!("expected execution failure, got {other:?}"),
        }
    }

    #[test]
    fn connection_refused_is_a_failure_the_engine_survives() {
        // Port 1 on loopback: nothing listens there.
        let mut executor = executor("http://127.0.0.1:1/hook");
        let error = executor.execute(&request(), 1).unwrap_err();
        assert!(matches!(error, ActionError::ExecutionFailed { .. }));
    }

    #[test]
    fn timeout_bounded_by_configured_timeout_ms() {
        // A listener that accepts but never responds: the request must
        // return within the configured timeout, not hang the worker.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let _ = listener.accept(); // accept, then deliberately stall
            std::thread::sleep(Duration::from_secs(30));
        });
        let mut executor = WebhookExecutor::new(WebhookConfig {
            url: address,
            method: WebhookMethod::Post,
            headers: vec![],
            timeout_ms: 300,
        })
        .unwrap();
        let started = std::time::Instant::now();
        let result = executor.execute(&request(), 1);
        assert!(result.is_err(), "expected timeout error");
        assert!(
            started.elapsed() < Duration::from_millis(2_000),
            "timeout not bounded: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn put_and_patch_methods_are_configurable() {
        assert_eq!(WebhookMethod::default().as_str(), "POST");
        assert_eq!(WebhookMethod::Put.as_str(), "PUT");
        assert_eq!(WebhookMethod::Patch.as_str(), "PATCH");
    }
}
