//! One flag set on an interface.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A single interface flag, as the kernel names it.
///
/// `UP`, `BROADCAST`, `MULTICAST`, `LOOPBACK`, `LOWER_UP`, `NO-CARRIER`, `POINTOPOINT`.
///
/// `UP` and `LOWER_UP` are worth telling apart: the first is administrative and the second
/// is whether there is a carrier, so a cable pulled out of a NIC that is still configured
/// up shows as `LOWER_UP` disappearing and `NO-CARRIER` arriving.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterfaceFlag(NonEmptyText);

impl InterfaceFlag {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "interface flag")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&InterfaceFlag> for Observation {
    fn from(value: &InterfaceFlag) -> Self {
        Observation::text(value.as_str())
    }
}
