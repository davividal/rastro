//! What kind of link an interface is.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The link type, in the word the kernel prints.
///
/// `ether`, `loopback`, `none` for a tunnel, `sit`, `ppp`. Text rather than an enum: the
/// set is the kernel's ARPHRD table, which has dozens of entries and gains more with each
/// hardware family Linux supports.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkType(NonEmptyText);

impl LinkType {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "link type")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&LinkType> for Observation {
    fn from(value: &LinkType) -> Self {
        Observation::text(value.as_str())
    }
}
