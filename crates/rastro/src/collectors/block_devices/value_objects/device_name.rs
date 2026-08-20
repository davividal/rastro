//! What a block device is called.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A block device's name, as the kernel calls it.
///
/// `sda`, `sda1`, `dm-0`, `nvme0n1p2`. Unique across the box, which is what makes it the
/// key this facet is filed under.
///
/// **Not a path.** `lsblk` reports `sda`, not `/dev/sda`, and the two are not
/// interchangeable: a device-mapper device is `dm-0` here while the path an operator uses
/// is a symlink under `/dev/mapper`. Recording the name the kernel uses keeps this facet
/// joinable with `/proc/partitions` and with the `mounts` facet's device column.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceName(NonEmptyText);

impl DeviceName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "device name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&DeviceName> for Observation {
    fn from(value: &DeviceName) -> Self {
        Observation::text(value.as_str())
    }
}
