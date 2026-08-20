//! Where a device is mounted.

use rastro_collector::{AbsolutePath, CollectionError, Observation};

/// One place a device is mounted.
///
/// **A device can have several, and `lsblk` reports a list for exactly that reason.** A
/// bind mount, or a btrfs subvolume mounted twice, gives one device two mount points. The
/// list also legitimately contains a JSON `null` for a device that is mounted nowhere,
/// which is `lsblk`'s way of writing an empty list and is the case that catches a naive
/// reader.
///
/// An [`AbsolutePath`] underneath, because a mount point always is one, and a relative
/// value would mean the column was read out of the wrong field.
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

impl From<&MountPoint> for Observation {
    fn from(point: &MountPoint) -> Self {
        Observation::text(point.as_str())
    }
}
