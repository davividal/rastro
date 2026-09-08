//! How the engine stacks an image's layers.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The graph driver the engine resolved: `overlayfs`, `overlay2`, `btrfs`, `vfs`.
///
/// Worth a value of its own because a driver change is a rebuild of every layer on the box,
/// and because `vfs` where `overlay2` was is the signature of a kernel or mount option that
/// stopped supporting the fast path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StorageDriver(NonEmptyText);

impl StorageDriver {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "storage driver")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&StorageDriver> for Observation {
    fn from(driver: &StorageDriver) -> Self {
        Observation::text(driver.as_str())
    }
}
