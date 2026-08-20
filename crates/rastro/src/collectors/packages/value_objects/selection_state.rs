//! What the operator asked for.

use rastro_collector::{CollectionError, NonEmptyText};

/// What is *wanted* for a package: `install`, `hold`, `deinstall`, `purge`, `unknown`.
///
/// The word comes from dpkg rather than from a table here, so a vocabulary dpkg extends
/// needs no change in rastro and no letter can be met with a refusal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectionState(NonEmptyText);

impl SelectionState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "selection state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
