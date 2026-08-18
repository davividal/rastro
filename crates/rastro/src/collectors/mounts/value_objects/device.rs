//! What is mounted.

use rastro_collector::{CollectionError, NonEmptyText};

/// A block device, a pseudo-filesystem's name, or an overlay's label.
///
/// Not a path, even when it looks like one. `proc`, `tmpfs` and `overlay` are all
/// legal, so the only invariant is that the host named something.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Device(NonEmptyText);

impl Device {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "device")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
