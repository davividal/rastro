//! What a database is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name of a database in a cluster.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DatabaseName(NonEmptyText);

impl DatabaseName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "database name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
