//! A Linux capability a container was given or denied.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A capability name as the engine reports it: `NET_ADMIN`, `SYS_TIME`, `CHOWN`.
///
/// **Not normalised, and not checked against the kernel's list.** docker accepts several
/// spellings and reports back what it was given, and the kernel's set grows with each
/// release, so a name rastro has not heard of is still a capability this container has.
/// Refusing one would cost the facet a container it otherwise described perfectly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Capability(NonEmptyText);

impl Capability {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "capability")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&Capability> for Observation {
    fn from(capability: &Capability) -> Self {
        Observation::text(capability.as_str())
    }
}
