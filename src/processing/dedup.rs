//! Bounded deduplication of events by producer-supplied id.

use std::collections::{HashMap, VecDeque};

use crate::processing::PipelineConfigError;

/// Bounded, capacity-limited cache of event ids.
///
/// A duplicate is an [`Event`](crate::Event) id already present; duplicates
/// are dropped. Eviction is FIFO by first insertion: when capacity is
/// exceeded the oldest id is forgotten, and a replayed id after eviction is
/// treated as new — the explicit, documented trade-off of bounded memory.
/// Only ids are stored, never events.
#[derive(Debug)]
pub struct Deduplicator {
    seen: HashMap<String, ()>,
    order: VecDeque<String>,
    capacity: usize,
}

impl Deduplicator {
    /// Creates a deduplicator holding at most `capacity` ids.
    pub fn new(capacity: usize) -> Result<Self, PipelineConfigError> {
        if capacity == 0 {
            return Err(PipelineConfigError::InvalidCapacity {
                field: "dedup_capacity",
                value: capacity,
            });
        }
        Ok(Self {
            seen: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        })
    }

    /// Returns `true` when the id has not been seen before (and records it).
    ///
    /// Returns `false` for a duplicate.
    pub fn is_new(&mut self, id: &str) -> bool {
        if self.seen.contains_key(id) {
            return false;
        }
        if self.seen.len() == self.capacity {
            self.evict_oldest();
        }
        self.seen.insert(id.to_string(), ());
        self.order.push_back(id.to_string());
        true
    }

    /// Forgets the oldest recorded id. Only called when the map is full.
    fn evict_oldest(&mut self) {
        if let Some(oldest) = self.order.pop_front() {
            self.seen.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dedup(capacity: usize) -> Deduplicator {
        Deduplicator::new(capacity).unwrap()
    }

    #[test]
    fn first_id_passes() {
        let mut dedup = dedup(10);
        assert!(dedup.is_new("evt-1"));
    }

    #[test]
    fn immediate_replay_is_dropped() {
        let mut dedup = dedup(10);
        assert!(dedup.is_new("evt-1"));
        assert!(!dedup.is_new("evt-1"));
    }

    #[test]
    fn distinct_ids_pass() {
        let mut dedup = dedup(10);
        assert!(dedup.is_new("evt-1"));
        assert!(dedup.is_new("evt-2"));
        assert!(dedup.is_new("evt-3"));
    }

    #[test]
    fn oldest_id_is_evicted_at_capacity_and_replay_passes_again() {
        let mut dedup = dedup(2);
        assert!(dedup.is_new("a"));
        assert!(dedup.is_new("b"));
        // Inserting `c` evicts `a` (FIFO by first insertion).
        assert!(dedup.is_new("c"));
        // `a` was evicted, so a replay is treated as new — and its
        // re-insertion evicts the next-oldest id, `b`.
        assert!(dedup.is_new("a"));
        assert!(dedup.is_new("b"));
    }

    #[test]
    fn capacity_one_edge() {
        let mut dedup = dedup(1);
        assert!(dedup.is_new("a"));
        assert!(!dedup.is_new("a"));
        assert!(dedup.is_new("b")); // evicts "a"
        assert!(!dedup.is_new("b"));
        assert!(dedup.is_new("a")); // was evicted
    }

    #[test]
    fn zero_capacity_is_rejected() {
        let err = Deduplicator::new(0).unwrap_err();
        assert_eq!(
            err,
            PipelineConfigError::InvalidCapacity {
                field: "dedup_capacity",
                value: 0
            }
        );
    }
}
