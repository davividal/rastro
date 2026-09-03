//! One setting on a pool member.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// What an `upstream` `server` line says after its address: `weight=3`, `max_fails=2`,
/// `backup`, `down`.
///
/// **`down` and `backup` are the two that matter most and the two that look least like
/// state.** A member marked `down` is out of the pool, which is exactly the kind of change
/// somebody makes during an incident and forgets, and it lives in one word at the end of a
/// line no other facet reads.
///
/// Sorted, because nginx reads them as a set.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServerParameter(NonEmptyText);

impl ServerParameter {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "upstream server parameter")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ServerParameter> for Observation {
    fn from(parameter: &ServerParameter) -> Self {
        Observation::text(parameter.as_str())
    }
}
