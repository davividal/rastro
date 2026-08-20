//! How far an address reaches.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// An address's scope, in the word `ip` prints.
///
/// `global`, `link`, `host`, `site`. It is the difference between an address reachable from
/// the network and one reachable only from the box, which makes it the field on an address
/// most worth diffing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressScope(NonEmptyText);

impl AddressScope {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "address scope")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&AddressScope> for Observation {
    fn from(value: &AddressScope) -> Self {
        Observation::text(value.as_str())
    }
}
