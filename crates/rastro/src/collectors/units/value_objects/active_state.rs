//! Whether a unit is running.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// Whether the unit is running.
///
/// `active`, `reloading`, `inactive`, `failed`, `activating`, `deactivating` or
/// `maintenance`. `failed` is the one an operator scans a diff for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActiveState(NonEmptyText);

impl ActiveState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "active state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ActiveState> for Observation {
    fn from(value: &ActiveState) -> Self {
        Observation::text(value.as_str())
    }
}
