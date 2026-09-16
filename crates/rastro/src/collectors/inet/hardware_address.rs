//! An interface's link-layer address.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A link-layer address, as the kernel prints it.
///
/// Usually a MAC address. Text rather than six parsed bytes, because the kernel prints
/// link-layer addresses of several widths here: `00:00:00:00:00:00` for the loopback, which
/// has none, twenty bytes for InfiniBand, and nothing at all for a tunnel. A six-byte type
/// would refuse the interfaces it did not expect.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HardwareAddress(NonEmptyText);

impl HardwareAddress {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "hardware address")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&HardwareAddress> for Observation {
    fn from(value: &HardwareAddress) -> Self {
        Observation::text(value.as_str())
    }
}
