//! Optional persistence (spec 12 / decision 002): adapters behind a
//! contract, never in the in-memory processing path.
//!
//! This module ships the local-file snapshot adapter. Crash-consistency:
//! snapshots write to a temp file in the same directory and `rename` into
//! place — a crash mid-write leaves the previous snapshot intact. A
//! corrupted or partial snapshot is rejected on restore, never silently
//! loaded.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::event::Event;
use crate::state::{StateEntry, StateKey, StateStore, StateValue};

/// Persistence errors: actionable, non-fatal to the engine (invariant 8
/// pattern — persistence failures are surfaced, never crash processing).
#[derive(Debug)]
pub enum PersistenceError {
    /// The snapshot file could not be written.
    WriteFailed {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The snapshot file could not be read.
    ReadFailed {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The snapshot exists but cannot be parsed (corruption).
    Corrupted {
        /// The path that failed.
        path: PathBuf,
        /// The parse failure.
        reason: String,
    },
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::WriteFailed { path, source } => {
                write!(f, "failed to write snapshot {}: {source}", path.display())
            }
            PersistenceError::ReadFailed { path, source } => {
                write!(f, "failed to read snapshot {}: {source}", path.display())
            }
            PersistenceError::Corrupted { path, reason } => {
                write!(f, "snapshot {} is corrupted: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for PersistenceError {}

/// What a restore returns: a snapshot to load, or nothing (no file yet).
#[derive(Debug, Default)]
pub struct Snapshot {
    /// The restored entries.
    pub entries: Vec<StateEntry>,
}

/// The persistence contract. The engine never depends on a concrete
/// adapter — the runtime configuration layer wires one when configured.
pub trait PersistenceAdapter: std::fmt::Debug + Send {
    /// Persists the current state. Must be atomic in effect: either the
    /// previous snapshot or a complete new one survives a crash.
    fn snapshot(&mut self, store: &StateStore) -> Result<(), PersistenceError>;
    /// Loads the last snapshot; `Ok(None)` when none exists.
    fn restore(&self) -> Result<Option<Snapshot>, PersistenceError>;
}

/// Wire format for snapshots. State types are deliberately serde-free
/// (spec 04); the adapter owns the mapping, keeping the runtime-state
/// types decoupled from any persistence format.
#[derive(Debug, Serialize, Deserialize)]
struct SnapshotEntry {
    source: String,
    kind: String,
    value: SnapshotValue,
    previous_value: Option<SnapshotValue>,
    updated_at_ms: i64,
    updates: u64,
}

#[derive(Debug, Serialize, Deserialize)]
enum SnapshotValue {
    #[serde(rename = "numeric")]
    Numeric(f64),
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "boolean")]
    Boolean(bool),
    #[serde(rename = "structured")]
    Structured(serde_json::Value),
}

impl From<&StateValue> for SnapshotValue {
    fn from(value: &StateValue) -> Self {
        match value {
            StateValue::Numeric(v) => SnapshotValue::Numeric(*v),
            StateValue::Text(v) => SnapshotValue::Text(v.clone()),
            StateValue::Boolean(v) => SnapshotValue::Boolean(*v),
            StateValue::Structured(v) => SnapshotValue::Structured(v.clone()),
        }
    }
}

impl TryFrom<SnapshotValue> for StateValue {
    type Error = String;

    fn try_from(value: SnapshotValue) -> Result<Self, Self::Error> {
        Ok(match value {
            SnapshotValue::Numeric(v) => StateValue::Numeric(v),
            SnapshotValue::Text(v) => StateValue::Text(v),
            SnapshotValue::Boolean(v) => StateValue::Boolean(v),
            SnapshotValue::Structured(v) => StateValue::Structured(v),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotFile {
    /// Format version (bump on incompatible changes; restores of unknown
    /// versions fail loudly, never guess).
    format_version: u32,
    entries: Vec<SnapshotEntry>,
}

const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Local file snapshot adapter: atomic write (temp + rename), strict
/// version check on read.
#[derive(Debug, Clone)]
pub struct LocalFileAdapter {
    path: PathBuf,
}

impl LocalFileAdapter {
    /// Points the adapter at a snapshot file path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn serialize(store: &StateStore) -> SnapshotFile {
        SnapshotFile {
            format_version: SNAPSHOT_FORMAT_VERSION,
            entries: store
                .entries()
                .iter()
                .map(|entry| SnapshotEntry {
                    source: entry.key.source.clone(),
                    kind: entry.key.kind.clone(),
                    value: SnapshotValue::from(&entry.value),
                    previous_value: entry.previous_value.as_ref().map(SnapshotValue::from),
                    updated_at_ms: entry.updated_at_ms,
                    updates: entry.updates,
                })
                .collect(),
        }
    }

    fn deserialize(file: SnapshotFile, path: &Path) -> Result<Snapshot, PersistenceError> {
        if file.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(PersistenceError::Corrupted {
                path: path.to_path_buf(),
                reason: format!(
                    "unsupported format version {} (expected {})",
                    file.format_version, SNAPSHOT_FORMAT_VERSION
                ),
            });
        }
        let mut entries = Vec::with_capacity(file.entries.len());
        for entry in file.entries {
            let value = StateValue::try_from(entry.value).map_err(|reason| {
                PersistenceError::Corrupted {
                    path: path.to_path_buf(),
                    reason,
                }
            })?;
            let previous_value = match entry.previous_value {
                Some(previous) => Some(StateValue::try_from(previous).map_err(|reason| {
                    PersistenceError::Corrupted {
                        path: path.to_path_buf(),
                        reason,
                    }
                })?),
                None => None,
            };
            entries.push(StateEntry {
                key: StateKey::new(entry.source, entry.kind),
                value,
                previous_value,
                updated_at_ms: entry.updated_at_ms,
                updates: entry.updates,
            });
        }
        Ok(Snapshot { entries })
    }
}

impl PersistenceAdapter for LocalFileAdapter {
    fn snapshot(&mut self, store: &StateStore) -> Result<(), PersistenceError> {
        let file = Self::serialize(store);
        let temp = self.path.with_extension("tmp");
        // Atomic-in-effect: write the temp file fully, then rename over the
        // target. A crash before rename leaves the old snapshot intact.
        let serialized =
            serde_json::to_vec(&file).map_err(|error| PersistenceError::Corrupted {
                path: self.path.clone(),
                reason: format!("serialize failed: {error}"),
            })?;
        std::fs::write(&temp, serialized).map_err(|source| PersistenceError::WriteFailed {
            path: temp.clone(),
            source,
        })?;
        std::fs::rename(&temp, &self.path).map_err(|source| PersistenceError::WriteFailed {
            path: self.path.clone(),
            source,
        })
    }

    fn restore(&self) -> Result<Option<Snapshot>, PersistenceError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.path).map_err(|source| PersistenceError::ReadFailed {
            path: self.path.clone(),
            source,
        })?;
        let file: SnapshotFile =
            serde_json::from_slice(&bytes).map_err(|error| PersistenceError::Corrupted {
                path: self.path.clone(),
                reason: error.to_string(),
            })?;
        Self::deserialize(file, &self.path).map(Some)
    }
}

/// Loads a snapshot into a fresh store (restore path used by the runtime).
/// Entry count is capped by the store's capacity automatically — restore
/// cannot violate the bounded-state invariant.
pub fn apply_snapshot(store: &mut StateStore, snapshot: &Snapshot) {
    for entry in &snapshot.entries {
        store.restore_entry(entry.clone());
    }
}

/// Observable persistence counters (invariant 8: degradation is visible).
/// Shared between the runtime wiring and `/v1/status`.
#[derive(Debug, Default)]
pub struct PersistenceCounters {
    /// Successful periodic snapshots.
    pub snapshots_ok: std::sync::atomic::AtomicU64,
    /// Failed snapshots (engine continues; the count is the alarm).
    pub snapshot_failures: std::sync::atomic::AtomicU64,
    /// Snapshots restored at startup (0 or 1 per process).
    pub restored: std::sync::atomic::AtomicU64,
}

/// Historical event storage contract (optional; e.g. PostgreSQL). Writes
/// are *history only*: the runtime state path never blocks on a sink
/// (spec 12) — callers batch through a bounded buffer and degrade to
/// drop-and-count on overflow or failure (invariant 8 pattern).
pub trait HistoricalSink: std::fmt::Debug + Send {
    /// Persists one batch of events. Failures are returned, never panics.
    fn write_batch(&mut self, events: &[Event]) -> Result<(), PersistenceError>;
}

/// Reference sink: keeps the latest `capacity` events in memory. Used for
/// tests and as the degradation reference implementation — it documents
/// exactly what "drop-and-count, never fatal" means for real sinks.
#[derive(Debug)]
pub struct InMemoryHistoricalSink {
    capacity: usize,
    events: std::collections::VecDeque<Event>,
    written: u64,
    dropped: u64,
    failures: u64,
}

impl InMemoryHistoricalSink {
    /// Creates a bounded sink. Capacity 0 is rejected: a sink that can
    /// never accept is a configuration error, not a runtime event.
    pub fn new(capacity: usize) -> Result<Self, PersistenceError> {
        if capacity == 0 {
            return Err(PersistenceError::Corrupted {
                path: std::path::PathBuf::from("<memory>"),
                reason: "historical sink capacity must be > 0".to_string(),
            });
        }
        Ok(Self {
            capacity,
            events: std::collections::VecDeque::with_capacity(capacity),
            written: 0,
            dropped: 0,
            failures: 0,
        })
    }

    /// Events currently retained (bounded by capacity).
    pub fn backlog(&self) -> usize {
        self.events.len()
    }

    /// Total events accepted.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Total events dropped on overflow — observable, never fatal.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Total failed `write_batch` calls recorded via [`Self::record_failure`]
    /// by wrappers that translate sink errors into degradation.
    pub fn failures(&self) -> u64 {
        self.failures
    }

    /// Counts a failed batch (call from adapter glue when a real sink
    /// errors): the batch is lost but the engine continues.
    pub fn record_failure(&mut self, batch_len: usize) {
        self.failures += 1;
        self.dropped += batch_len as u64;
    }
}

impl HistoricalSink for InMemoryHistoricalSink {
    fn write_batch(&mut self, events: &[Event]) -> Result<(), PersistenceError> {
        for event in events {
            if self.events.len() == self.capacity {
                self.events.pop_front();
                self.dropped += 1;
            }
            self.events.push_back(event.clone());
            self.written += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, Payload};
    use crate::processing::Output;
    use crate::state::StateConfig;

    fn store() -> StateStore {
        StateStore::new(StateConfig::default()).unwrap()
    }

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

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("deltu-persist-{name}-{}", std::process::id()))
    }

    #[test]
    fn snapshot_round_trip_preserves_entries() {
        let path = temp_path("roundtrip");
        let mut source = store();
        source.observe(&Output::Event(text_event("e1", 1_000, "open")));
        source.observe(&Output::Event(text_event("e2", 2_000, "closed")));

        let mut adapter = LocalFileAdapter::new(&path);
        adapter.snapshot(&source).unwrap();

        let mut target = store();
        let snapshot = adapter.restore().unwrap().expect("snapshot exists");
        apply_snapshot(&mut target, &snapshot);

        let entry = target
            .get(&StateKey::new("sensor-1", "door.state"))
            .unwrap();
        assert_eq!(entry.value, StateValue::Text("closed".into()));
        assert_eq!(entry.previous_value, Some(StateValue::Text("open".into())));
        assert_eq!(entry.updated_at_ms, 2_000);
        assert_eq!(entry.updates, 2);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_snapshot_restores_none() {
        let adapter = LocalFileAdapter::new(temp_path("missing"));
        let snapshot = adapter.restore().unwrap();
        assert!(snapshot.is_none());
    }

    #[test]
    fn crash_mid_write_leaves_previous_snapshot_intact() {
        let path = temp_path("atomic");
        let mut adapter = LocalFileAdapter::new(&path);

        // Write a known-good snapshot.
        let mut source = store();
        source.observe(&Output::Event(text_event("e1", 1_000, "open")));
        adapter.snapshot(&source).unwrap();

        // Simulate a crash mid-write: a truncated temp file exists while
        // the real snapshot is untouched. A subsequent restore must load
        // the previous complete snapshot, not the partial temp.
        std::fs::write(path.with_extension("tmp"), b"{partial").unwrap();
        let snapshot = adapter.restore().unwrap().expect("previous snapshot");
        assert_eq!(snapshot.entries.len(), 1);
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(path.with_extension("tmp")).ok();
    }

    #[test]
    fn corrupted_snapshot_is_rejected_not_loaded() {
        let path = temp_path("corrupt");
        std::fs::write(&path, b"{ not a snapshot }").unwrap();
        let adapter = LocalFileAdapter::new(&path);
        match adapter.restore() {
            Err(PersistenceError::Corrupted { .. }) => {}
            other => panic!("expected corruption error, got {other:?}"),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unknown_format_version_is_rejected() {
        let path = temp_path("version");
        let file = SnapshotFile {
            format_version: 999,
            entries: vec![],
        };
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let adapter = LocalFileAdapter::new(&path);
        match adapter.restore() {
            Err(PersistenceError::Corrupted { reason, .. }) => {
                assert!(reason.contains("version"))
            }
            other => panic!("expected version error, got {other:?}"),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn bounded_sink_drops_and_counts_never_fails() {
        let mut sink = InMemoryHistoricalSink::new(2).unwrap();
        let events: Vec<Event> = (0..5)
            .map(|i| text_event(&format!("e{i}"), 1_000 + i, "x"))
            .collect();
        // Overflow drops the oldest — and the write itself still succeeds:
        // degradation is counted, never fatal (invariant 8 pattern).
        sink.write_batch(&events).unwrap();
        assert_eq!(sink.written(), 5);
        assert_eq!(sink.dropped(), 3);
        assert_eq!(sink.backlog(), 2);
        let _ = sink;
    }

    #[test]
    fn zero_capacity_sink_is_a_configuration_error() {
        assert!(InMemoryHistoricalSink::new(0).is_err());
    }

    #[test]
    fn failed_batches_are_counted_not_fatal() {
        let mut sink = InMemoryHistoricalSink::new(4).unwrap();
        sink.record_failure(3);
        assert_eq!(sink.failures(), 1);
        assert_eq!(sink.dropped(), 3);
        // The engine continues: the sink is usable afterwards.
        sink.write_batch(&[text_event("e1", 1, "x")]).unwrap();
        assert_eq!(sink.written(), 1);
    }

    #[test]
    fn restore_cannot_violate_store_capacity() {
        let path = temp_path("capacity");
        // A store with capacity 2 snapshotting 2 entries...
        let mut source = StateStore::new(StateConfig {
            max_entries: 2,
            expire_after_ms: None,
        })
        .unwrap();
        for (id, kind, ts) in [("a", "one", 100), ("b", "two", 200)] {
            let event = Event::new(
                id,
                "s",
                kind,
                ts,
                Payload::Text {
                    value: "v".to_string(),
                },
            )
            .unwrap();
            source.observe(&Output::Event(event));
        }
        let mut adapter = LocalFileAdapter::new(&path);
        adapter.snapshot(&source).unwrap();

        // ...restored into a capacity-1 store: the store's own eviction
        // applies, capacity is never exceeded.
        let mut tiny = StateStore::new(StateConfig {
            max_entries: 1,
            expire_after_ms: None,
        })
        .unwrap();
        let snapshot = adapter.restore().unwrap().unwrap();
        apply_snapshot(&mut tiny, &snapshot);
        assert!(tiny.len() <= 1);
        assert_eq!(tiny.counters().evicted, 1);
        std::fs::remove_file(&path).ok();
    }
}
