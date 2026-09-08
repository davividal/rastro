//! A moment an engine reported.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A timestamp exactly as the engine printed it: `2026-09-08T11:00:21.648460605Z`.
///
/// **Kept as text rather than converted to a number.** The filesystem walker records
/// nanoseconds since the epoch because it reads them from `stat`, where that is what a
/// timestamp is. An engine hands out RFC 3339 with nanosecond precision, and turning that
/// into an integer would mean rastro parsing calendars: a dependency, an offset to get wrong,
/// and a value a reader can no longer check against `docker inspect`. It is stable, it sorts,
/// and it diffs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EngineInstant(NonEmptyText);

impl EngineInstant {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "engine timestamp")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&EngineInstant> for Observation {
    fn from(instant: &EngineInstant) -> Self {
        Observation::text(instant.as_str())
    }
}
