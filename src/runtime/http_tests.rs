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
    let scheduler = crate::runtime::scheduler::Scheduler::new(Arc::new(tx.clone()));
    (
        AppState {
            core,
            queue: Arc::new(tx),
            queue_capacity,
            mqtt_counters: Arc::new(crate::input::mqtt::SharedMqttCounters::new()),
            metrics: Arc::new(Mutex::new(crate::metrics::MetricsRegistry::new())),
            started: Arc::new(Instant::now()),
            max_batch,
            persistence_counters: Arc::new(crate::persistence::PersistenceCounters::default()),
            scheduler: Arc::new(tokio::sync::Mutex::new(scheduler)),
            scheduler_snapshot_path: None,
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

/// A valid interval-schedule POST body.
fn interval_body(id: &str) -> String {
    serde_json::json!({
        "id": id,
        "spec": { "type": "interval", "unit": "minutes", "every": 30 },
        "timezone": "Asia/Kolkata",
        "payload": { "flow": "nightly-sync" }
    })
    .to_string()
}

#[tokio::test]
async fn schedule_crud_round_trip() {
    let (state, _rx) = test_state(10, 100);
    let a = app(state);

    // create
    let (status, body) = send(a.clone(), "POST", "/v1/schedules", Some(interval_body("nightly"))).await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");

    // list shows it with next-run info
    let (status, body) = send(a.clone(), "GET", "/v1/schedules", None).await;
    assert_eq!(status, StatusCode::OK);
    let schedules = body["data"]["schedules"].as_array().unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0]["config"]["id"], "nightly");
    assert!(schedules[0]["next_run_ms"].as_i64().is_some(), "next run exposed");
    assert_eq!(schedules[0]["running"], true);
    assert_eq!(body["data"]["active_count"], 1);

    // single get
    let (status, body) = send(a.clone(), "GET", "/v1/schedules/nightly", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["config"]["spec"]["type"], "interval");

    // stop → running=false, next_run hidden
    let (status, body) = send(a.clone(), "POST", "/v1/schedules/nightly/stop", None).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["running"], false);    let (status, body) = send(a.clone(), "GET", "/v1/schedules/nightly", None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["next_run_ms"].is_null(), "stopped schedule has no next run");

    // start → running=true again
    let (status, _body) = send(a.clone(), "POST", "/v1/schedules/nightly/start", None).await;
    assert_eq!(status, StatusCode::OK);    let (status, body) = send(a.clone(), "GET", "/v1/schedules/nightly", None).await;

    assert_eq!(body["data"]["running"], true);

    // delete
    let (status, _body) = send(a.clone(), "DELETE", "/v1/schedules/nightly", None).await;
    assert_eq!(status, StatusCode::OK);
    let (_status, body) = send(a, "GET", "/v1/schedules", None).await;
    assert_eq!(body["data"]["schedules"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn schedule_validation_errors_are_documented_envelopes() {
    let (state, _rx) = test_state(10, 100);
    let a = app(state);

    // bad timezone
    let bad_tz = serde_json::json!({
        "id": "x",
        "spec": { "type": "daily", "time": "09:00" },
        "timezone": "Mars/Olympus"
    })
    .to_string();
    let (status, body) = send(a.clone(), "POST", "/v1/schedules", Some(bad_tz)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_schedule");

    // malformed body
    let (status, body) = send(a, "POST", "/v1/schedules", Some("{nope".into())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_schedule");
}

#[tokio::test]
async fn schedule_unknown_id_returns_404() {
    let (state, _rx) = test_state(10, 100);
    let a = app(state);
    let (status, body) = send(a.clone(), "GET", "/v1/schedules/ghost", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "schedule_not_found");
    let (status, _body) = send(a.clone(), "POST", "/v1/schedules/ghost/stop", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _body) = send(a.clone(), "POST", "/v1/schedules/ghost/start", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _body) = send(a, "POST", "/v1/schedules/ghost/trigger", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn schedule_trigger_now_enqueues_real_event() {
    let (state, mut rx) = test_state(10, 100);
    let a = app(state);

    let (status, _body) = send(a.clone(), "POST", "/v1/schedules", Some(interval_body("tick"))).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _body) = send(a.clone(), "POST", "/v1/schedules/tick/trigger", None).await;
    assert_eq!(status, StatusCode::OK);

    // The manual trigger delivers an event through the queue with the
    // manual flag set.
    let item = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("manual trigger must enqueue an event")
        .expect("queue closed");
    let event = &item.events[0];
    assert_eq!(event.kind, "schedule.fired");
    match &event.payload {
        crate::event::Payload::Json { value } => {
            assert_eq!(value["manual"], true);
            assert_eq!(value["flow"], "nightly-sync");
        }
        other => panic!("expected json payload, got {other:?}"),
    }
    let _ = item.reply.send(CoreOutcome {
        accepted: vec![true],
        action_outcomes: vec![],
    });

    // Status reflects the manual firing.
    let (status, body) = send(a, "GET", "/v1/schedules/tick", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["last_fired_ms"].as_i64().is_some());
    assert_eq!(body["data"]["fired_count"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn schedule_interval_task_fires_real_event_through_queue() {
    // End-to-end: a 1-second interval schedule must deliver a real event
    // with kind `schedule.fired` through the same bounded queue as HTTP
    // ingestion — no heartbeat, no polling.
    let (state, mut rx) = test_state(10, 100);
    let a = app(state);

    let body = serde_json::json!({
        "id": "fast",
        "spec": { "type": "interval", "unit": "seconds", "every": 1 },
        "timezone": "UTC",
        "payload": { "hello": "world" }
    })
    .to_string();
    let (status, _body) = send(a, "POST", "/v1/schedules", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);

    let item = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("schedule did not fire within 5s")
        .expect("queue closed");
    assert_eq!(item.events.len(), 1);
    let event = &item.events[0];
    assert_eq!(event.kind, "schedule.fired");
    assert_eq!(event.source, "deltu://scheduler");
    assert!(event.id.starts_with("sched-fast-"));
    let payload = match &event.payload {
        crate::event::Payload::Json { value } => value.clone(),
        other => panic!("expected json payload, got {other:?}"),
    };
    assert_eq!(payload["hello"], "world");
    let _ = item.reply.send(CoreOutcome {
        accepted: vec![true],
        action_outcomes: vec![],
    });
}

#[tokio::test]
async fn stopped_schedule_never_fires() {
    let (state, mut rx) = test_state(10, 100);
    // Keep a sender alive: once the router state is consumed the channel
    // would otherwise close and end the wait early.
    let keep_alive = state.queue.clone();
    let a = app(state);

    // Created disabled: no task ever spawns.
    let body = serde_json::json!({
        "id": "paused",
        "spec": { "type": "interval", "unit": "seconds", "every": 1 },
        "timezone": "UTC",
        "enabled": false
    })
    .to_string();
    let (status, _body) = send(a.clone(), "POST", "/v1/schedules", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);

    // A started-then-stopped schedule with a 1-hour cadence cannot fire
    // between the two calls; after stop, nothing may arrive either.
    let body = serde_json::json!({
        "id": "hourly",
        "spec": { "type": "interval", "unit": "hours", "every": 1 },
        "timezone": "UTC"
    })
    .to_string();
    let (status, _body) = send(a.clone(), "POST", "/v1/schedules", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = send(a, "POST", "/v1/schedules/hourly/stop", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["running"], false);

    // Nothing arrives: the disabled schedules never spawn tasks.
    let nothing = tokio::time::timeout(std::time::Duration::from_millis(1_500), rx.recv()).await;
    assert!(nothing.is_err(), "stopped schedule fired");
    drop(keep_alive);
}

#[tokio::test]
async fn multiple_schedules_run_independently() {
    let (state, mut rx) = test_state(10, 100);
    let a = app(state);

    for id in ["one", "two"] {
        let body = serde_json::json!({
            "id": id,
            "spec": { "type": "interval", "unit": "seconds", "every": 1 },
            "timezone": "UTC"
        })
        .to_string();
        let (status, _body) = send(a.clone(), "POST", "/v1/schedules", Some(body)).await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let mut seen = std::collections::HashSet::new();
    for _ in 0..2 {
        let item = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("schedule did not fire")
            .expect("queue closed");
        let event = &item.events[0];
        assert_eq!(event.kind, "schedule.fired");
        seen.insert(
            event
                .id
                .strip_prefix("sched-")
                .and_then(|rest| rest.rsplit_once('-').map(|(id, _)| id.to_string()))
                .unwrap_or_default(),
        );
        let _ = item.reply.send(CoreOutcome {
            accepted: vec![true],
            action_outcomes: vec![],
        });
    }
    assert!(seen.contains("one"), "both schedules fired independently: {seen:?}");
    assert!(seen.contains("two"), "both schedules fired independently: {seen:?}");
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
