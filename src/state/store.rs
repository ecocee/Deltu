//! The state store: keyed current values written from pipeline outputs,
//! read by rules, expired periodically with an injected clock.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::event::{Event, Payload};
use crate::processing::{Aggregated, Output};
use crate::state::{StateConfig, StateCounters, StateError};

/// The store's key — mirrors the pipeline keying exactly:
/// `(source, kind)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateKey {
    pub source: String,
    pub kind: String,
}

impl StateKey {
    pub fn new(source: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            kind: kind.into(),
        }
    }
}

/// The current value of a key. Mirrors [`Payload`](crate::event::Payload)
/// without duplicating its serde tagging — state values are read by rules,
/// not reserialized (spec 04: no serde derives here).
#[derive(Debug, Clone, PartialEq)]
pub enum StateValue {
    Numeric(f64),
    Text(String),
    Boolean(bool),
    Structured(JsonValue),
}

impl StateValue {
    /// Numeric view for rule comparisons; `None` for non-numeric values.
    pub fn as_numeric(&self) -> Option<f64> {
        match self {
            StateValue::Numeric(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<&Payload> for StateValue {
    fn from(payload: &Payload) -> Self {
        match payload {
            Payload::Numeric { value } => StateValue::Numeric(*value),
            Payload::Text { value } => StateValue::Text(value.clone()),
            Payload::Boolean { value } => StateValue::Boolean(*value),
            Payload::Json { value } => StateValue::Structured(value.clone()),
        }
    }
}

/// One key's current snapshot. Only the last transition is retained —
/// never a history.
#[derive(Debug, Clone, PartialEq)]
pub struct StateEntry {
    pub key: StateKey,
    /// Current value.
    pub value: StateValue,
    /// Immediate previous value, for transition rules (e.g. open → closed).
    pub previous_value: Option<StateValue>,
    /// Event-time (milliseconds) of the last update to this key.
    pub updated_at_ms: i64,
    /// Number of updates applied to this key since creation.
    pub updates: u64,
}

/// Internal record: the entry plus its insertion sequence for the
/// stalest-first tie-break (spec 04: smallest `updated_at_ms` wins, ties
/// broken by earliest insertion).
#[derive(Debug)]
struct Record {
    entry: StateEntry,
    inserted_seq: u64,
}

/// The in-memory current-state store.
///
/// Clock-free and deterministic: expiration receives `now_ms` from the
/// caller (the runtime unit schedules the periodic calls). Capacity bounds
/// the store regardless of input volume; when an insert would exceed
/// `max_entries`, the **stalest** entry (smallest `updated_at_ms`, ties by
/// earliest insertion) is evicted — deliberately least-recently-updated
/// rather than Unit 03's FIFO, because fresh data outranks old here.
#[derive(Debug)]
pub struct StateStore {
    entries: HashMap<StateKey, Record>,
    capacity: usize,
    expire_after_ms: Option<i64>,
    next_seq: u64,
    counters: StateCounters,
}

impl StateStore {
    /// Validates the configuration and creates an empty store.
    pub fn new(config: StateConfig) -> Result<Self, StateError> {
        if config.max_entries == 0 {
            return Err(StateError::InvalidCapacity(config.max_entries));
        }
        if let Some(expire_after) = config.expire_after_ms
            && expire_after <= 0
        {
            return Err(StateError::InvalidExpireAfter(expire_after));
        }
        Ok(Self {
            entries: HashMap::with_capacity(config.max_entries),
            capacity: config.max_entries,
            expire_after_ms: config.expire_after_ms,
            next_seq: 0,
            counters: StateCounters::default(),
        })
    }

    /// Applies one pipeline output to the store (`observe` is the seam
    /// between Unit 03 and this unit; the runtime wires it permanently).
    pub fn observe(&mut self, output: &Output) {
        match output {
            Output::Event(event) => self.observe_event(event),
            Output::Aggregated(summary) => self.observe_aggregated(summary),
        }
    }

    /// Raw (non-numeric) events update their key directly with
    /// `updated_at_ms = event.timestamp`. Spec 03 routes all numeric events
    /// into aggregation, so a numeric payload cannot reach this path; if it
    /// ever does (caller misuse), it is still stored honestly rather than
    /// dropped silently.
    fn observe_event(&mut self, event: &Event) {
        let key = StateKey::new(event.source.clone(), event.kind.clone());
        let value = StateValue::from(&event.payload);
        self.apply(key, value, event.timestamp);
    }

    /// Aggregated summaries store the window mean, current as of the
    /// window's end.
    fn observe_aggregated(&mut self, summary: &Aggregated) {
        let key = StateKey::new(summary.source.clone(), summary.kind.clone());
        self.apply(
            key,
            StateValue::Numeric(summary.mean),
            summary.window_end_ms,
        );
    }

    /// Shared upsert: create with `updates = 1`, or move the current value
    /// into `previous_value`, increment `updates`, and refresh recency.
    /// Enforces the capacity cap before inserting a new key.
    fn apply(&mut self, key: StateKey, value: StateValue, updated_at_ms: i64) {
        if let Some(record) = self.entries.get_mut(&key) {
            record.entry.previous_value = Some(record.entry.value.clone());
            record.entry.value = value;
            record.entry.updated_at_ms = updated_at_ms;
            record.entry.updates += 1;
            return;
        } // New key: enforce the cap first. Victim = stalest entry: smallest
        // updated_at_ms, ties broken by earliest insertion.
        if self.entries.len() == self.capacity
            && let Some(victim) = self
                .entries
                .iter()
                .min_by(|(_, a), (_, b)| {
                    a.entry
                        .updated_at_ms
                        .cmp(&b.entry.updated_at_ms)
                        .then(a.inserted_seq.cmp(&b.inserted_seq))
                })
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&victim);
            self.counters.evicted += 1;
        }

        self.entries.insert(
            key.clone(),
            Record {
                entry: StateEntry {
                    key,
                    value,
                    previous_value: None,
                    updated_at_ms,
                    updates: 1,
                },
                inserted_seq: self.next_seq,
            },
        );
        self.next_seq += 1;
    }

    /// Read one key's current snapshot.
    pub fn get(&self, key: &StateKey) -> Option<&StateEntry> {
        self.entries.get(key).map(|record| &record.entry)
    }

    /// All entries, sorted by `(source, kind)` so downstream evaluation is
    /// deterministic regardless of internal hash order.
    pub fn entries(&self) -> Vec<&StateEntry> {
        let mut records: Vec<&StateEntry> =
            self.entries.values().map(|record| &record.entry).collect();
        records.sort_by(|a, b| (&a.key.source, &a.key.kind).cmp(&(&b.key.source, &b.key.kind)));
        records
    }

    /// Number of retained entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the store holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Periodic scan-and-evict with an injected clock. An entry expires
    /// when `now_ms - updated_at_ms >= expire_after_ms`; expired entries
    /// are removed and returned with their final values so "went offline"
    /// logic can use the last known state. Negative ages (producer clock
    /// skew) are never expired. Returns an empty Vec when expiration is
    /// disabled.
    pub fn expire(&mut self, now_ms: i64) -> Vec<StateEntry> {
        let Some(expire_after) = self.expire_after_ms else {
            return Vec::new();
        };

        let expired_keys: Vec<StateKey> = self
            .entries
            .iter()
            .filter(|(_, record)| {
                let age = now_ms - record.entry.updated_at_ms;
                age >= expire_after
            })
            .map(|(key, _)| key.clone())
            .collect();

        let mut expired = Vec::with_capacity(expired_keys.len());
        for key in expired_keys {
            if let Some(record) = self.entries.remove(&key) {
                expired.push(record.entry);
            }
        }
        self.counters.expired += expired.len() as u64;
        expired
    }

    /// Counter snapshot.
    pub fn counters(&self) -> &StateCounters {
        &self.counters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> StateStore {
        StateStore::new(StateConfig::default()).unwrap()
    }

    fn text_event(id: &str, ts: i64, value: &str) -> Event {
        text_event_on(id, "sensor-1", "door.state", ts, value)
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

    fn summary(source: &str, kind: &str, window_end: i64, mean: f64) -> Aggregated {
        Aggregated {
            source: source.to_string(),
            kind: kind.to_string(),
            window_start_ms: window_end - 1_000,
            window_end_ms: window_end,
            count: 2,
            min: mean - 1.0,
            max: mean + 1.0,
            sum: mean * 2.0,
            mean,
            last: mean + 1.0,
        }
    }

    // --- Write path ---

    #[test]
    fn raw_text_event_creates_entry_with_event_timestamp() {
        let mut store = store();
        store.observe(&Output::Event(text_event("e1", 5_000, "open")));
        let entry = store.get(&StateKey::new("sensor-1", "door.state")).unwrap();
        assert_eq!(entry.value, StateValue::Text("open".into()));
        assert_eq!(entry.updated_at_ms, 5_000);
        assert_eq!(entry.updates, 1);
        assert_eq!(entry.previous_value, None);
    }

    #[test]
    fn raw_boolean_event_creates_entry() {
        let mut store = store();
        let event = Event::new(
            "e1",
            "sensor-1",
            "door.contact",
            100,
            Payload::Boolean { value: true },
        )
        .unwrap();
        store.observe(&Output::Event(event));
        let entry = store
            .get(&StateKey::new("sensor-1", "door.contact"))
            .unwrap();
        assert_eq!(entry.value, StateValue::Boolean(true));
    }

    #[test]
    fn raw_structured_event_creates_entry() {
        let mut store = store();
        let event = Event::new(
            "e1",
            "sensor-1",
            "device.status",
            100,
            Payload::Json {
                value: serde_json::json!({ "battery": 97 }),
            },
        )
        .unwrap();
        store.observe(&Output::Event(event));
        let entry = store
            .get(&StateKey::new("sensor-1", "device.status"))
            .unwrap();
        assert_eq!(
            entry.value,
            StateValue::Structured(serde_json::json!({ "battery": 97 }))
        );
    }

    #[test]
    fn aggregated_summary_stores_mean_with_window_end_timestamp() {
        let mut store = store();
        store.observe(&Output::Aggregated(summary(
            "sensor-1",
            "temperature.reading",
            61_000,
            21.5,
        )));
        let entry = store
            .get(&StateKey::new("sensor-1", "temperature.reading"))
            .unwrap();
        assert_eq!(entry.value, StateValue::Numeric(21.5));
        assert_eq!(entry.updated_at_ms, 61_000); // window_end_ms, not start
        assert_eq!(entry.updates, 1);
    }

    #[test]
    fn reobservation_sets_previous_value_increments_updates_refreshes_recency() {
        let mut store = store();
        store.observe(&Output::Event(text_event("e1", 1_000, "open")));
        store.observe(&Output::Event(text_event("e2", 2_000, "closed")));
        let entry = store.get(&StateKey::new("sensor-1", "door.state")).unwrap();
        assert_eq!(entry.value, StateValue::Text("closed".into()));
        assert_eq!(entry.previous_value, Some(StateValue::Text("open".into())));
        assert_eq!(entry.updated_at_ms, 2_000);
        assert_eq!(entry.updates, 2);
    }

    // --- Read path ---

    #[test]
    fn get_misses_unknown_key() {
        let store = store();
        assert!(store.get(&StateKey::new("nope", "nope")).is_none());
    }

    #[test]
    fn entries_are_sorted_by_source_then_kind() {
        let mut store = store();
        // Insert in non-sorted order.
        for (source, kind, ts) in [
            ("s-b", "k-2", 100),
            ("s-a", "k-2", 100),
            ("s-b", "k-1", 100),
            ("s-a", "k-1", 100),
        ] {
            let event = Event::new(
                "id",
                source,
                kind,
                ts,
                Payload::Text {
                    value: "v".to_string(),
                },
            )
            .unwrap();
            store.observe(&Output::Event(event));
        }
        let keys: Vec<(String, String)> = store
            .entries()
            .into_iter()
            .map(|e| (e.key.source.clone(), e.key.kind.clone()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("s-a".to_string(), "k-1".to_string()),
                ("s-a".to_string(), "k-2".to_string()),
                ("s-b".to_string(), "k-1".to_string()),
                ("s-b".to_string(), "k-2".to_string()),
            ]
        );
    }

    #[test]
    fn len_and_is_empty() {
        let mut store = store();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        store.observe(&Output::Event(text_event("e1", 100, "open")));
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    // --- Expiration ---

    #[test]
    fn entry_past_limit_is_removed_returned_and_counted() {
        let mut store = store(); // expire_after 300_000
        // Two distinct keys: one stale, one fresh.
        store.observe(&Output::Event(text_event("e1", 1_000, "open")));
        store.observe(&Output::Event(text_event_on(
            "e2",
            "sensor-1",
            "light.state",
            250_000,
            "on",
        )));

        let expired = store.expire(301_001);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].key, StateKey::new("sensor-1", "door.state"));
        assert_eq!(expired[0].value, StateValue::Text("open".into()));
        assert_eq!(store.counters().expired, 1);
        assert_eq!(store.len(), 1); // fresh light.state survives
        assert!(
            store
                .get(&StateKey::new("sensor-1", "light.state"))
                .is_some()
        );
    }

    #[test]
    fn boundary_age_expires() {
        let mut store = store();
        store.observe(&Output::Event(text_event("e1", 1_000, "open")));
        // age == expire_after_ms exactly (301_000 - 1_000 == 300_000).
        let expired = store.expire(301_000);
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn fresh_entry_survives() {
        let mut store = store();
        store.observe(&Output::Event(text_event("e1", 300_000, "open")));
        assert!(store.expire(300_100).is_empty()); // age 100 < 300_000
        assert_eq!(store.len(), 1);
        assert_eq!(store.counters().expired, 0);
    }

    #[test]
    fn expiration_can_be_disabled() {
        let mut store = StateStore::new(StateConfig {
            expire_after_ms: None,
            ..StateConfig::default()
        })
        .unwrap();
        store.observe(&Output::Event(text_event("e1", 1_000, "open")));
        assert!(store.expire(10_000_000).is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn negative_age_never_expires() {
        let mut store = StateStore::new(StateConfig {
            expire_after_ms: Some(1_000),
            ..StateConfig::default()
        })
        .unwrap();
        store.observe(&Output::Event(text_event("e1", 5_000, "open")));
        // now before the entry's timestamp (clock skew): age is negative.
        assert!(store.expire(4_999).is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn reobserved_expired_key_starts_fresh() {
        let mut store = StateStore::new(StateConfig {
            expire_after_ms: Some(1_000),
            ..StateConfig::default()
        })
        .unwrap();
        store.observe(&Output::Event(text_event("e1", 1_000, "open")));
        store.observe(&Output::Event(text_event("e2", 1_100, "closed"))); // updates = 2
        let _ = store.expire(3_000);
        assert!(store.is_empty());

        store.observe(&Output::Event(text_event("e3", 3_100, "ajar")));
        let entry = store.get(&StateKey::new("sensor-1", "door.state")).unwrap();
        assert_eq!(entry.value, StateValue::Text("ajar".into()));
        assert_eq!(entry.updates, 1);
        assert_eq!(entry.previous_value, None);
    }

    // --- Capacity ---

    #[test]
    fn inserting_beyond_capacity_evicts_stalest_and_counts() {
        let mut store = StateStore::new(StateConfig {
            max_entries: 2,
            expire_after_ms: None,
        })
        .unwrap();

        // Three distinct keys; the first observed has the stalest timestamp.
        for (source, kind, ts) in [("s", "old", 100), ("s", "middle", 200), ("s", "new", 300)] {
            let event = Event::new(
                "id",
                source,
                kind,
                ts,
                Payload::Text {
                    value: "v".to_string(),
                },
            )
            .unwrap();
            store.observe(&Output::Event(event));
        }

        assert_eq!(store.len(), 2);
        assert_eq!(store.counters().evicted, 1);
        assert!(store.get(&StateKey::new("s", "old")).is_none());
        assert!(store.get(&StateKey::new("s", "middle")).is_some());
        assert!(store.get(&StateKey::new("s", "new")).is_some());
    }

    #[test]
    fn tie_break_evicts_earliest_inserted() {
        let mut store = StateStore::new(StateConfig {
            max_entries: 2,
            expire_after_ms: None,
        })
        .unwrap();

        // Same timestamp: insertion order decides.
        for (id, kind, ts) in [
            ("i1", "first", 100),
            ("i2", "second", 100),
            ("i3", "third", 100),
        ] {
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
            store.observe(&Output::Event(event));
        }

        assert_eq!(store.counters().evicted, 1);
        assert!(store.get(&StateKey::new("s", "first")).is_none());
        assert!(store.get(&StateKey::new("s", "second")).is_some());
        assert!(store.get(&StateKey::new("s", "third")).is_some());
    }

    #[test]
    fn updated_entry_refreshes_recency_and_survives() {
        let mut store = StateStore::new(StateConfig {
            max_entries: 2,
            expire_after_ms: None,
        })
        .unwrap();

        store.observe(&Output::Event(text_event("e1", 100, "open"))); // sensor-1/door.state
        store.observe(&Output::Aggregated(summary("s", "temp", 150, 20.0)));
        // Refresh the oldest key so `temp` becomes the stalest.
        store.observe(&Output::Event(text_event("e2", 200, "closed")));

        // Third key forces eviction; `temp` (ts 150) must be the victim,
        // not the refreshed `door.state` (ts 200).
        let event = Event::new(
            "e3",
            "s",
            "third.key",
            300,
            Payload::Text {
                value: "v".to_string(),
            },
        )
        .unwrap();
        store.observe(&Output::Event(event));

        assert_eq!(store.counters().evicted, 1);
        assert!(store.get(&StateKey::new("s", "temp")).is_none());
        assert!(
            store
                .get(&StateKey::new("sensor-1", "door.state"))
                .is_some()
        );
    }

    // --- Config ---

    #[test]
    fn zero_capacity_is_rejected() {
        assert_eq!(
            StateStore::new(StateConfig {
                max_entries: 0,
                expire_after_ms: None
            })
            .unwrap_err(),
            StateError::InvalidCapacity(0)
        );
    }

    #[test]
    fn non_positive_expire_after_is_rejected() {
        assert_eq!(
            StateStore::new(StateConfig {
                max_entries: 10,
                expire_after_ms: Some(0)
            })
            .unwrap_err(),
            StateError::InvalidExpireAfter(0)
        );
        assert_eq!(
            StateStore::new(StateConfig {
                max_entries: 10,
                expire_after_ms: Some(-1)
            })
            .unwrap_err(),
            StateError::InvalidExpireAfter(-1)
        );
    }

    #[test]
    fn defaults_are_conservative_and_documented() {
        let config = StateConfig::default();
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.expire_after_ms, Some(300_000));
    }

    // --- End-to-end within the unit ---

    #[test]
    fn realistic_output_sequence_produces_expected_state() {
        let mut store = store();

        // A text event (door opens).
        store.observe(&Output::Event(text_event("d1", 1_000, "open")));
        // A closed-window summary (temperature mean over [60_000, 61_000)).
        store.observe(&Output::Aggregated(summary(
            "sensor-1",
            "temperature.reading",
            61_000,
            21.5,
        )));
        // The door closes again (transition).
        store.observe(&Output::Event(text_event("d2", 61_500, "closed")));

        assert_eq!(store.len(), 2);
        let door = store.get(&StateKey::new("sensor-1", "door.state")).unwrap();
        assert_eq!(door.value, StateValue::Text("closed".into()));
        assert_eq!(door.previous_value, Some(StateValue::Text("open".into())));
        let temperature = store
            .get(&StateKey::new("sensor-1", "temperature.reading"))
            .unwrap();
        assert_eq!(temperature.value, StateValue::Numeric(21.5));
        assert_eq!(temperature.updated_at_ms, 61_000);
    }
}
