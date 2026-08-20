//! What filesystem is on a device.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The filesystem on a device, in the word `lsblk` prints.
///
/// `ext4`, `vfat`, `swap`, `LVM2_member`, `crypto_LUKS`.
///
/// **This overlaps the `mounts` facet's type of the same name, and the two are read from
/// different places.** There, it is what the kernel says is *mounted*; here, it is what
/// `blkid` finds *on the device*, whether or not anything mounted it. A partition holding
/// an unmounted ext4 filesystem appears here and not there, and that difference is the
/// reason both exist.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilesystemType(NonEmptyText);

impl FilesystemType {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "filesystem type")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&FilesystemType> for Observation {
    fn from(value: &FilesystemType) -> Self {
        Observation::text(value.as_str())
    }
}
