//! Who an access rule or a trust setting is about.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The subject of an `allow`, a `deny` or a `set_real_ip_from`: an address, a CIDR range,
/// `unix:`, or `all`.
///
/// Not parsed into an address type. `all` and `unix:` are not addresses at all, a CIDR range
/// is not one either, and a fingerprint compares this value with the same rule's value on
/// another run rather than reasoning about what it contains.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressPattern(NonEmptyText);

impl AddressPattern {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "address pattern")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&AddressPattern> for Observation {
    fn from(pattern: &AddressPattern) -> Self {
        Observation::text(pattern.as_str())
    }
}
