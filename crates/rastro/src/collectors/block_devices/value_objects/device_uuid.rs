//! A device's stable identifier.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A UUID, either of the filesystem or of the partition.
///
/// **The identifier `/etc/fstab` and the boot loader actually refer to**, which makes it
/// among the most useful values in the facet: a partition reformatted in place keeps its
/// name and its size and gets a new filesystem UUID, so this is what says the data is gone.
///
/// Text rather than a parsed UUID, because not every value here is one: a vfat filesystem's
/// identifier is a 32-bit volume id that `lsblk` prints as `AB66-741E`, and a swap area or
/// an LVM member uses forms of its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceUuid(NonEmptyText);

impl DeviceUuid {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "device uuid")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&DeviceUuid> for Observation {
    fn from(value: &DeviceUuid) -> Self {
        Observation::text(value.as_str())
    }
}
