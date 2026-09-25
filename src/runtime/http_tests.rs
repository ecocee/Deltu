//! HTTP integration tests via `tower::ServiceExt::oneshot` (spec 07):
//! every documented status code covered without a live socket.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // oneshot — dev-dependency via axum's tower stack

use crate::actions::{ActionDefinition, ActionDispatcher, ActionKind, LogLevel};
use crate::processing::PipelineConfig;
use crate::rules::RuleEngine;
use crate::runtime::http::{AppState, WorkItem, router};
#[allow(unused_imports)]
use crate::runtime::worker::CoreOutcome;
use crate::runtime::worker::EngineCore;
use crate::state::{StateConfig, StateStore};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn test_state(
    queue_capacity: usize,
    max_batch: usize,
) -> (AppState, tokio::sync::mpsc::Receiver<WorkItem>) {
    let pipeline = PipelineConfig::default();
    let pipeline = crate::ProcessingPipeline::new(pipeline).unwrap();
    let state = StateStore::new(StateConfig::default()).unwrap();
    let rules = RuleEngine::new(vec![]).unwrap();
    let actions = ActionDispatcher::new(vec![ActionDefinition {
        id: "log-ops".to_string(),
        kind: ActionKind::Log {
            level: LogLevel::Info,
            template: None,
        },
    }])
    .unwrap();
    let core = Arc::new(Mutex::new(EngineCore::new(pipeline, state, rules, actions)));
    let (tx, rx) = tokio::sync::mpsc::channel(queue_capacity);
    (
        AppState {
            core,
            queue: Arc::new(tx),
            queue_capacity,
            mqtt_counters: Arc::new(crate::input::mqtt::SharedMqttCounters::new()),
            metrics: Arc::new(Mutex::new(crate::metrics::MetricsRegistry::new())),
            started: Arc::new(Instant::now()),
            max_batch,
        },
        rx,
    )
}

fn app(state: AppState) -> Router {
    router(state, 1_048_576)
}

async fn send(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> (StatusCode, serde_json::Value) {
    let (status, bytes) = send_raw(app, method, uri, body).await;
    let value: serde_json::Value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::String(
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    };
    (status, value)
}

async fn send_raw(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> (StatusCode, axum::body::Bytes) {
    let builder = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_048_576)
        .await
        .unwrap();
    (status, bytes)
}

fn valid_event(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "source": "test/source",
        "kind": "test.event",
        "timestamp": 1_769_412_000_123i64,
        "payload": { "type": "numeric", "value": 1.0 }
    })
}

#[tokio::test]
async fn health_answers_without_touching_engine() {
    let (state, _rx) = test_state(10, 100);
    let (status, body) = send(app(state), "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn valid_batch_returns_202_with_accepted() {
    let (state, mut rx) = test_state(10, 100);
    // Serve the worker side of the queue inline.
    let worker_state = state.clone();
    tokio::spawn(async move {
        while let Some(item) = rx.recv().await {
            let core = worker_state.core.lock().unwrap();
            let _ = core.state_entries(); // touch to prove access
            let _ = item.reply.send(CoreOutcome {
                accepted: vec![true; item.events.len()],
                action_outcomes: vec![],
            });
        }
    });

    let body = serde_json::json!({ "events": [valid_event("e1"), valid_event("e2")] });
    let (status, body) = send(app(state), "POST", "/v1/events", Some(body.to_string())).await;
    assert_eq!(status, StatusCode::ACCEPTED, "body: {body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["accepted"], serde_json::json!([true, true]));
}

#[tokio::test]
async fn malformed_json_returns_400_with_envelope() {
    let (state, _rx) = test_state(10, 100);
    let (status, body) = send(
        app(state),
        "POST",
        "/v1/events",
        Some("{not json".to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "invalid_event");
    assert!(!body["error"]["message"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn missing_events_field_returns_400() {
    let (state, _rx) = test_state(10, 100);
    let (status, body) = send(
        app(state),
        "POST",
        "/v1/events",
        Some(serde_json::json!({ "nope": [] }).to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("events")
    );
}

#[tokio::test]
async fn empty_batch_returns_400() {
    let (state, _rx) = test_state(10, 100);
    let (status, _) = send(
        app(state),
        "POST",
        "/v1/events",
        Some(serde_json::json!({ "events": [] }).to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn batch_over_max_returns_400() {
    let (state, _rx) = test_state(10, 2); // max_batch = 2
    let body = serde_json::json!({
        "events": [valid_event("e1"), valid_event("e2"), valid_event("e3")]
    });
    let (status, body) = send(app(state), "POST", "/v1/events", Some(body.to_string())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("maximum")
    );
}

#[tokio::test]
async fn invalid_event_fails_validation_with_named_index() {
    let (state, _rx) = test_state(10, 100);
    let mut event = valid_event("e1");
    event["timestamp"] = serde_json::json!(-5); // invalid per spec 02
    let body = serde_json::json!({ "events": [event] });
    let (status, body) = send(app(state), "POST", "/v1/events", Some(body.to_string())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("events[0]")
    );
}

#[tokio::test]
async fn oversized_body_returns_413() {
    let (state, _rx) = test_state(10, 100);
    let app = crate::runtime::http::router(state, 16); // 16-byte limit
    let body = serde_json::json!({ "events": [valid_event("e1")] }).to_string();
    // The body-limit layer rejects before the handler; assert status only
    // (its rejection body is not the JSON envelope).
    let (status, _) = send_raw(app, "POST", "/v1/events", Some(body)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn queue_full_returns_429() {
    // Capacity-1 queue, no worker draining it: first request may succeed
    // (waits for a reply that never comes — dropped with the receiver),
    // so fill the queue then expect 429 on the next.
    let (state, _rx) = test_state(1, 100);
    let app = app(state);

    let request = || {
        let app = app.clone();
        let body = serde_json::json!({ "events": [valid_event("e1")] }).to_string();
        async move { send(app, "POST", "/v1/events", Some(body)).await }
    };

    // First request: sent to queue, handler awaits reply. The receiver is
    // not polled, so the queue slot is still occupied. Response will be
    // whichever the runtime schedules; the important assertion is the
    // *second* request.
    let first = tokio::spawn(request());
    tokio::task::yield_now().await;
    let (status, body) = request().await;
    // With capacity 1 occupied and no drain, this must be 429.
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "body: {body}");

    first.abort(); // first request never resolves without a worker
}

#[tokio::test]
async fn status_reports_counters_shape() {
    let (state, _rx) = test_state(10, 100);
    let (status, body) = send(app(state), "GET", "/v1/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    let data = &body["data"];
    assert!(data["version"].as_str().is_some());
    assert!(data["uptime_seconds"].as_u64().is_some());
    assert!(data["pipeline"]["filtered_out"].as_u64().is_some());
    assert!(data["state"]["entries"].as_u64().is_some());
    assert!(data["actions"]["attempted"].as_u64().is_some());
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let (state, _rx) = test_state(10, 100);
    let (status, _) = send(app(state), "GET", "/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
