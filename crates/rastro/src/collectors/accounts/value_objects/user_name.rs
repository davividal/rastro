//! What a user is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// A login name, as the account database spells it.
///
/// The name and not the number is what identifies an account here, because the
/// number is the thing that gets reused: delete a user and create another and the
/// next `useradd` hands out the same uid. Two fingerprints keyed by uid would call
/// that one account with a new name, where keying by name calls it what it is, one
/// account gone and another arrived.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserName(NonEmptyText);

impl UserName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "user name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
