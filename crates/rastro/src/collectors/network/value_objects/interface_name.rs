//! What a network interface is called.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// An interface name, as the kernel reports it.
///
/// `lo`, `enp0s8`, `br-1a2b3c`, `eth0.100` for a VLAN.
///
/// **The name identifies an interface here, not its index**, for the reason a user name is
/// preferred over a uid: an index is assigned at boot in device-enumeration order and can
/// be handed to a different interface after a reboot, while the name is what every
/// configuration file refers to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterfaceName(NonEmptyText);

impl InterfaceName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "interface name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&InterfaceName> for Observation {
    fn from(value: &InterfaceName) -> Self {
        Observation::text(value.as_str())
    }
}
