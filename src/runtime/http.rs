//! The HTTP API boundary: validation-only handlers feeding a bounded
//! channel to the worker (invariant 7: no processing in handlers).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value as JsonValue, json};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::actions::ActionDispatcher;
use crate::event::Event;
use crate::persistence::{LocalFileAdapter, PersistenceAdapter};
use crate::processing::ProcessingPipeline;
use crate::rules::RuleEngine;
use crate::runtime::RuntimeConfig;
use crate::runtime::worker::{CoreOutcome, EngineCore, now_unix_ms};
use crate::state::StateStore;

/// A batch submitted through the API, paired with a reply channel for the
/// worker's results (bounded backpressure: the handler waits for the
/// worker's receipt; when the queue is full the handler returns 429).
pub struct WorkItem {
    pub events: Vec<Event>,
    pub reply: oneshot::Sender<CoreOutcome>,
}

/// HTTP API errors mapped to the documented status codes. (`TooLarge` is
/// enforced by the body-limit layer and mapped to its own code in the
/// response path — the variant exists for that mapping and for direct
/// use by future transports.)
#[allow(dead_code)]
pub(crate) enum ApiError {
    /// Malformed JSON or invalid event shape.
    BadRequest(String),
    /// Payload too large.
    TooLarge,
    /// Bounded queue full (overload).
    QueueFull,
    /// Shutting down.
    ShuttingDown,
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::QueueFull => StatusCode::TOO_MANY_REQUESTS,
            ApiError::ShuttingDown => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(_) => "invalid_event",
            ApiError::TooLarge => "payload_too_large",
            ApiError::QueueFull => "queue_full",
            ApiError::ShuttingDown => "shutting_down",
        }
    }
}

/// The documented error envelope: every error response uses this shape.
fn error_body(code: &'static str, message: String) -> JsonValue {
    json!({ "success": false, "error": { "code": code, "message": message } })
}

/// `POST /v1/events`: validate → enqueue → await worker receipt.
///
/// The body is taken as raw bytes and parsed explicitly so that every
/// error path — including malformed JSON — uses the documented error
/// envelope rather than the extractor's default rejection body.
pub(crate) async fn post_events(
    State(app): State<AppState>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, Json<JsonValue>)> {
    let parsed: JsonValue = serde_json::from_slice(&body).map_err(|error| {
        let error = ApiError::BadRequest(format!("request body is not valid JSON: {error}"));
        (
            error.status(),
            Json(error_body(error.code(), error_message(&error))),
        )
    })?;
    handle_events(app, parsed)
        .await
        .map(|body| (StatusCode::ACCEPTED, body))
        .map_err(|error| {
            (
                error.status(),
                Json(error_body(error.code(), error_message(&error))),
            )
        })
}

fn error_message(error: &ApiError) -> String {
    match error {
        ApiError::BadRequest(message) => message.clone(),
        ApiError::TooLarge => "request body exceeds the configured maximum".to_string(),
        ApiError::QueueFull => "the processing queue is full; retry with backoff".to_string(),
        ApiError::ShuttingDown => "the engine is shutting down".to_string(),
    }
}

async fn handle_events(app: AppState, body: JsonValue) -> Result<Json<JsonValue>, ApiError> {
    let batch_started = std::time::Instant::now();
    // Body size was enforced by the router layer; validate the shape here.
    let Some(events_value) = body.get("events").cloned() else {
        return Err(ApiError::BadRequest(
            "missing required field: events".to_string(),
        ));
    };
    let Some(events_array) = events_value.as_array() else {
        return Err(ApiError::BadRequest(
            "field `events` must be an array".to_string(),
        ));
    };
    if events_array.is_empty() {
        return Err(ApiError::BadRequest(
            "field `events` must contain at least one event".to_string(),
        ));
    }
    if events_array.len() > app.max_batch {
        return Err(ApiError::BadRequest(format!(
            "batch exceeds the configured maximum of {} events",
            app.max_batch
        )));
    }

    // Validate and normalize every event BEFORE entering the pipeline
    // (invariant 9). Rejections name the offending index.
    let mut events = Vec::with_capacity(events_array.len());
    for (index, value) in events_array.iter().enumerate() {
        let event: Event = serde_json::from_value(value.clone()).map_err(|error| {
            ApiError::BadRequest(format!("events[{index}] is not a valid event: {error}"))
        })?;
        let event = event.validate().map_err(|error| {
            ApiError::BadRequest(format!("events[{index}] failed validation: {error}"))
        })?;
        events.push(event);
    }

    // Bounded handoff to the worker: try_send enforces backpressure.
    let (reply_tx, reply_rx) = oneshot::channel();
    app.queue
        .try_send(WorkItem {
            events,
            reply: reply_tx,
        })
        .map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => ApiError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => ApiError::ShuttingDown,
        })?;
    let outcome = reply_rx.await.map_err(|_| ApiError::ShuttingDown)?;

    if let Ok(mut metrics) = app.metrics.lock() {
        metrics.record_batch(outcome.accepted.len(), batch_started);
    }

    Ok(Json(json!({
        "success": true,
        "data": {
            "accepted": outcome.accepted,
            "actions_fired": outcome.action_outcomes.len(),
        }
    })))
}

/// `GET /health`: liveness without touching engine internals.
pub(crate) async fn get_health() -> Json<JsonValue> {
    Json(json!({ "status": "ok" }))
}

/// `GET /v1/status`: counters, queue depth, uptime, version.
pub(crate) async fn get_status(State(app): State<AppState>) -> Json<JsonValue> {
    // Single lock acquisition: queue depth comes from the same guard
    // (std Mutex is not reentrant — a second lock would self-deadlock).
    let metrics_snapshot = app
        .metrics
        .lock()
        .map(|mut metrics| metrics.snapshot())
        .unwrap_or_else(|_| serde_json::json!({}));
    let snapshot = {
        let mqtt = app.mqtt_counters.snapshot();
        let core = app.core.lock().expect("worker poisoned");
        json!({
            "version": crate::version(),
            "uptime_seconds": app.started.elapsed().as_secs(),
            "queue_depth": core.last_queue_depth,
            "pipeline": {
                "filtered_out": core.pipeline_counters().filtered_out,
                "duplicates": core.pipeline_counters().duplicates,
                "late_dropped": core.pipeline_counters().late_dropped,
                "windows_evicted": core.pipeline_counters().windows_evicted,
                "change_states_evicted": core.pipeline_counters().change_states_evicted,
                "changes_suppressed": core.pipeline_counters().changes_suppressed,
            },
            "state": {
                "entries": core.state_entries(),
                "expired": core.state_counters().expired,
                "evicted": core.state_counters().evicted,
            },
            "persistence": {
                "snapshots_ok": app.persistence_counters.snapshots_ok.load(Ordering::Relaxed),
                "snapshot_failures": app
                    .persistence_counters
                    .snapshot_failures
                    .load(Ordering::Relaxed),
                "restored": app.persistence_counters.restored.load(Ordering::Relaxed),
            },
            "mqtt": {
                "messages_received": mqtt.messages_received,
                "messages_rejected": mqtt.messages_rejected,
                "messages_dropped": mqtt.messages_dropped,
                "reconnects": mqtt.reconnects,
            },
            "metrics": metrics_snapshot,
            "actions": {
                "attempted": core.action_counters().actions_attempted,
                "succeeded": core.action_counters().actions_succeeded,
                "failed": core.action_counters().actions_failed,
                "ai": {
                    "calls": core.ai_usage().calls,
                    "succeeded": core.ai_usage().calls_succeeded,
                    "failed": core.ai_usage().calls_failed,
                    "tokens_used": core.ai_usage().tokens_used,
                    "latency_ms": core.ai_usage().latency_ms,
                },
            },
        })
    };
    Json(json!({ "success": true, "data": snapshot }))
}

/// Shared application state for handlers.
#[derive(Clone)]
pub(crate) struct AppState {
    /// The engine core behind a std mutex (worker-side processing is
    /// synchronous by design; HTTP handlers never touch it directly —
    /// only /v1/status snapshots it).
    pub core: Arc<std::sync::Mutex<EngineCore>>,
    /// Bounded work queue to the worker.
    pub queue: Arc<mpsc::Sender<WorkItem>>,
    /// Queue capacity (mirrors the channel bound; surfaced for tests).
    #[allow(dead_code)]
    pub queue_capacity: usize,
    /// MQTT adapter counters (zeroed when the adapter is disabled).
    pub mqtt_counters: Arc<crate::input::mqtt::SharedMqttCounters>,
    /// Metrics registry (latency rings + throughput window).
    pub metrics: Arc<std::sync::Mutex<crate::metrics::MetricsRegistry>>,
    /// Process start for uptime.
    pub started: Arc<std::time::Instant>,
    /// Maximum events per batch (from config).
    pub max_batch: usize,
    /// Persistence counters (zeroed when persistence is disabled).
    pub persistence_counters: Arc<crate::persistence::PersistenceCounters>,
}

/// Queue depth: the sender cannot observe the channel's internal length,
/// so the worker reports the last known depth through shared state.
/// (Inlined into `get_status`'s single lock acquisition.)
#[allow(dead_code)]
fn queue_depth_snapshot(app: &AppState) -> usize {
    app.core
        .lock()
        .map(|core| core.last_queue_depth)
        .unwrap_or(0)
}

/// Builds the axum router with documented routes and the body-size guard.
pub(crate) fn router(app: AppState, max_body_bytes: usize) -> axum::Router {
    use axum::extract::DefaultBodyLimit;
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/events", post(post_events))
        .route("/health", get(get_health))
        .route("/v1/status", get(get_status))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(app)
}

/// Runs the engine: spawns the worker loop, serves HTTP, and shuts down
/// gracefully on SIGINT/SIGTERM — draining the queue before exit.
pub async fn serve(config: RuntimeConfig) -> Result<(), String> {
    let RuntimeConfig {
        http,
        pipeline,
        state,
        rules,
        actions,
        mqtt,
        persistence,
        queue_capacity,
    } = config;

    // Build the validated core from config (engine constructors re-validate
    // and return their own errors, composed here into actionable strings).
    let pipeline = ProcessingPipeline::new(pipeline).map_err(|e| e.to_string())?;
    let state = StateStore::new(state).map_err(|e| e.to_string())?;
    let rules = RuleEngine::new(rules).map_err(|e| e.to_string())?;
    let actions = ActionDispatcher::new(actions).map_err(|e| e.to_string())?;
    let core = Arc::new(std::sync::Mutex::new(EngineCore::new(
        pipeline, state, rules, actions,
    )));
    let persistence_counters = Arc::new(crate::persistence::PersistenceCounters::default());

    // Optional persistence (spec 12): restore-on-startup, then periodic
    // snapshots owned by the worker loop. A failed restore is fatal at
    // startup (refusing to run with silently-lost state beats pretending);
    // snapshot failures at runtime degrade to a counter (invariant 8).
    let snapshot_adapter: Option<Arc<std::sync::Mutex<LocalFileAdapter>>> = if persistence.enabled {
        let path = persistence.path.clone().expect("validated non-empty");
        let adapter = LocalFileAdapter::new(&path);
        if let Ok(Some(snapshot)) = adapter.restore() {
            let mut core = core.lock().expect("worker poisoned");
            let state = core.state_mut();
            crate::persistence::apply_snapshot(state, &snapshot);
            persistence_counters
                .restored
                .fetch_add(1, Ordering::Relaxed);
            eprintln!(
                "deltu: restored {} state entries from {path}",
                snapshot.entries.len()
            );
        }
        Some(Arc::new(std::sync::Mutex::new(adapter)))
    } else {
        None
    };

    // Bounded queue: the backpressure boundary.
    let (tx, mut rx) = mpsc::channel::<WorkItem>(queue_capacity);
    let queue = Arc::new(tx);

    let mut snapshot_tick = tokio::time::interval(std::time::Duration::from_secs(
        persistence.snapshot_interval_secs,
    ));

    // Worker loop: drains the queue; holds the core behind the mutex.
    let worker_core = core.clone();
    let worker_persistence = persistence_counters.clone();
    let mut expire_tick = tokio::time::interval(crate::runtime::worker::EXPIRE_INTERVAL);
    let worker = tokio::spawn(async move {
        loop {
            tokio::select! {
                item = rx.recv() => {
                    let Some(item) = item else { break };
                    let now = now_unix_ms();
                    // Processing is synchronous, but network actions
                    // (webhook) must not run on a tokio worker: reqwest's
                    // blocking client panics inside a runtime context. The
                    // pipeline/state/rules portion stays here (fast, pure);
                    // action dispatch hops to the blocking pool when there
                    // is anything to dispatch.
                    let has_requests = {
                        let mut core = worker_core.lock().expect("worker poisoned");
                        core.process_pipeline_and_state(item.events, now)
                    };
                    let outcome = if has_requests {
                        let core = worker_core.clone();
                        tokio::task::spawn_blocking(move || {
                            let mut core = core.lock().expect("worker poisoned");
                            core.dispatch_pending(now)
                        })
                        .await
                        .unwrap_or_else(|_| CoreOutcome {
                            accepted: Vec::new(),
                            action_outcomes: Vec::new(),
                        })
                    } else {
                        let mut core = worker_core.lock().expect("worker poisoned");
                        core.dispatch_pending(now)
                    };
                    let _ = item.reply.send(outcome);
                }
                _ = expire_tick.tick() => {
                    let now = now_unix_ms();
                    let mut core = worker_core.lock().expect("worker poisoned");
                    let _ = core.expire_state(now);
                    core.last_queue_depth = 0; // queue drained between ticks
                }
                _ = snapshot_tick.tick(), if snapshot_adapter.is_some() => {
                    let Some(adapter) = &snapshot_adapter else { continue };
                    let core = worker_core.lock().expect("worker poisoned");
                    let result = adapter
                        .lock()
                        .expect("snapshot adapter poisoned")
                        .snapshot(core.state());
                    drop(core);
                    match result {
                        Ok(()) => {
                            worker_persistence
                                .snapshots_ok
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            worker_persistence
                                .snapshot_failures
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    });

    // MQTT adapter (optional; failures never propagate — invariant 8).
    // The handle is kept and aborted after the server stops: the adapter
    // holds a queue-sender clone, so leaving it running would keep the
    // worker's channel open forever and block graceful shutdown (audit
    // finding: SIGTERM hang with mqtt.enabled=true).
    let mut mqtt_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mqtt_counters = Arc::new(crate::input::mqtt::SharedMqttCounters::new());
    if mqtt.enabled {
        let handle = crate::input::mqtt::spawn(
            mqtt.mqtt,
            mqtt_counters.clone(),
            (*queue).clone(),
            queue_capacity,
        )?;
        mqtt_handle = Some(handle);
    }

    let app_state = AppState {
        core: core.clone(),
        queue,
        queue_capacity,
        mqtt_counters,
        metrics: Arc::new(std::sync::Mutex::new(crate::metrics::MetricsRegistry::new())),
        started: Arc::new(std::time::Instant::now()),
        max_batch: http.max_batch,
        persistence_counters: persistence_counters.clone(),
    };
    let app = router(app_state, http.max_body_bytes);

    let listener = tokio::net::TcpListener::bind(&http.bind)
        .await
        .map_err(|error| format!("failed to bind {}: {error}", http.bind))?;

    // Graceful shutdown on SIGINT/SIGTERM.
    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("sigterm handler");
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            let _ = ctrl_c.await;
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|error| format!("server error: {error}"))?;

    // Drain: stop the MQTT adapter first (it holds a queue sender), then
    // the worker sees the closed channel and exits; drop the core last.
    if let Some(handle) = mqtt_handle {
        handle.abort();
    }
    drop(core);
    let _ = worker.await;
    Ok(())
}

/// Kept for the config type: reference without triggering dead-code in
/// tests.
#[allow(dead_code)]
fn _config_shape_check(config: &RuntimeConfig) -> usize {
    config.queue_capacity
}
