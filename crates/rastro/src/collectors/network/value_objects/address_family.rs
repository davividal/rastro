//! Which family an address belongs to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// An address family, in the word `ip` prints.
///
/// `inet` or `inet6` for the two that matter here. Text rather than a two-variant enum,
/// because `ip` prints `link`, `mpls` and others for interface kinds this collector does
/// not exclude.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressFamily(NonEmptyText);

impl AddressFamily {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "address family")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&AddressFamily> for Observation {
    fn from(value: &AddressFamily) -> Self {
        Observation::text(value.as_str())
    }
}
