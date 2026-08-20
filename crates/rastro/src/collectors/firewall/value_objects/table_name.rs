//! Which table a chain belongs to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A packet-filter table.
///
/// `filter`, `nat`, `mangle`, `raw`, `security`. The table decides where in the kernel's
/// packet path a rule is consulted, so the same rule text in two tables does two different
/// things and the table is part of a rule's identity rather than a grouping convenience.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableName(NonEmptyText);

impl TableName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "table name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&TableName> for Observation {
    fn from(value: &TableName) -> Self {
        Observation::text(value.as_str())
    }
}
