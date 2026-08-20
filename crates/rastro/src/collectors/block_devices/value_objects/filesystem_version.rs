//! Which version of a filesystem is on a device.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The on-disk version of a filesystem.
///
/// `1.0` for ext4, `FAT16` and `FAT32` for vfat. Worth recording because it changes under
/// operations that look like no change at all: `tune2fs` enabling a feature moves it, and
/// so does a reformat that kept the same type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilesystemVersion(NonEmptyText);

impl FilesystemVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "filesystem version")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&FilesystemVersion> for Observation {
    fn from(value: &FilesystemVersion) -> Self {
        Observation::text(value.as_str())
    }
}
