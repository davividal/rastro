//! Where a filesystem is reachable.

use rastro_collector::{AbsolutePath, CollectionError};

/// The path a filesystem is mounted at.
///
/// Absoluteness is the whole invariant, and it is what distinguishes a mount point
/// from a [`Device`](super::device::Device) that happens to look like a path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MountPoint(AbsolutePath);

impl MountPoint {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(AbsolutePath::new(value, "mount point")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
