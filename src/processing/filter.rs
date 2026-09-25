//! Kind-based event filtering — the cheapest, most selective pipeline stage.

use crate::processing::PipelineConfigError;

/// Drops events whose `kind` is outside the configured allow-list.
///
/// `None` allows everything: the default configuration must not silently
/// drop data. An allow-list entry matches an event kind either exactly or as
/// its dotted family (`"temperature"` allows `temperature.reading` but not
/// `temperatures.x`).
#[derive(Debug, Clone)]
pub struct KindFilter {
    allowed: Option<Vec<String>>,
}

impl KindFilter {
    /// Creates a filter from the pipeline's `allowed_kinds` setting.
    ///
    /// Entries are trimmed and lowercased here so the hot path only
    /// compares; blank entries are rejected.
    pub fn new(allowed: Option<Vec<String>>) -> Result<Self, PipelineConfigError> {
        match allowed {
            None => Ok(Self { allowed: None }),
            Some(kinds) => {
                let mut normalized = Vec::with_capacity(kinds.len());
                for entry in kinds {
                    let entry = entry.trim().to_lowercase();
                    if entry.is_empty() {
                        return Err(PipelineConfigError::EmptyKindEntry);
                    }
                    normalized.push(entry);
                }
                Ok(Self {
                    allowed: Some(normalized),
                })
            }
        }
    }

    /// Returns `true` when the event kind passes the filter.
    pub fn allows(&self, kind: &str) -> bool {
        match &self.allowed {
            None => true,
            Some(entries) => {
                let kind = kind.to_lowercase();
                entries
                    .iter()
                    .any(|entry| super::kind_matches_family(&kind, entry))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(entries: &[&str]) -> KindFilter {
        KindFilter::new(Some(entries.iter().map(|s| s.to_string()).collect())).unwrap()
    }

    #[test]
    fn allows_everything_when_none() {
        let filter = KindFilter::new(None).unwrap();
        assert!(filter.allows("temperature.reading"));
        assert!(filter.allows("door.state"));
        assert!(filter.allows("anything.at.all"));
    }

    #[test]
    fn matches_exact_kind() {
        let filter = filter(&["temperature.reading"]);
        assert!(filter.allows("temperature.reading"));
        assert!(filter.allows("temperature.reading.raw")); // dotted family
        assert!(!filter.allows("temperature"));
        assert!(!filter.allows("temperaturex"));
    }

    #[test]
    fn matches_dotted_family_prefix() {
        let filter = filter(&["temperature"]);
        assert!(filter.allows("temperature.reading"));
        assert!(filter.allows("temperature"));
        assert!(filter.allows("Temperature.HUMIDITY")); // case-insensitive
    }

    #[test]
    fn sibling_prefix_does_not_match() {
        let filter = filter(&["temperature"]);
        // `temperatures.x` shares the character prefix but is not in the
        // `temperature` dotted family.
        assert!(!filter.allows("temperatures.x"));
    }

    #[test]
    fn disallowed_kind_is_dropped() {
        let filter = filter(&["temperature"]);
        assert!(!filter.allows("door.state"));
    }

    #[test]
    fn entries_are_trimmed_and_lowercased() {
        let filter = filter(&["  Temperature  "]);
        assert!(filter.allows("temperature.reading"));
    }

    #[test]
    fn blank_entry_is_rejected() {
        let err =
            KindFilter::new(Some(vec!["temperature".to_string(), "   ".to_string()])).unwrap_err();
        assert_eq!(err, PipelineConfigError::EmptyKindEntry);
    }
}
