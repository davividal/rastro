//! Which interface an address is scoped to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The interface an address is bound within, from the `%` suffix `ss` prints.
///
/// `127.0.0.53%lo` and `[fe80::...]%enp0s9` both appear on the development box. It is
/// mandatory for a link-local IPv6 address, which is ambiguous without one, and it is how
/// `systemd-resolved` pins its stub listener to the loopback interface.
///
/// A separate field rather than part of the address, because it is a separate fact: the
/// same address scoped to a different interface is a different binding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterfaceScope(NonEmptyText);

impl InterfaceScope {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "interface scope")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&InterfaceScope> for Observation {
    fn from(scope: &InterfaceScope) -> Self {
        Observation::text(scope.as_str())
    }
}
