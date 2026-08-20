//! What a group is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// A group name, as the account database spells it.
///
/// Keyed on for the same reason a [`UserName`](super::UserName) is: gids are reused
/// and names are what an operator recognises.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupName(NonEmptyText);

impl GroupName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "group name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
