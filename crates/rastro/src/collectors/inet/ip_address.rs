//! An address assigned to an interface.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// An IPv4 or IPv6 address, as the kernel prints it.
///
/// **Text rather than a parsed `IpAddr`, and deliberately not normalised.** An IPv6 address
/// has many legal spellings and the kernel picks one; re-rendering it through a parser risks
/// producing a different one and putting an address in the fingerprint that the host never
/// printed. The kernel's own spelling is stable between runs, which is all determinism
/// needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IpAddress(NonEmptyText);

impl IpAddress {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "ip address")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&IpAddress> for Observation {
    fn from(value: &IpAddress) -> Self {
        Observation::text(value.as_str())
    }
}
