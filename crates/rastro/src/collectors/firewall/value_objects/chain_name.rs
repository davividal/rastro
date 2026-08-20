//! Which chain a rule sits in.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A chain of rules.
///
/// The built-in chains are upper case and fixed per table: `INPUT`, `FORWARD`, `OUTPUT`,
/// `PREROUTING`, `POSTROUTING`. Anything else is a chain somebody created, and a
/// user-defined chain appearing is itself a change worth seeing, because rules elsewhere
/// must have been changed to jump into it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainName(NonEmptyText);

impl ChainName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "chain name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ChainName> for Observation {
    fn from(value: &ChainName) -> Self {
        Observation::text(value.as_str())
    }
}
