//! What kind of block device it is.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A block device's type, in the word `lsblk` prints.
///
/// `disk`, `part`, `lvm`, `crypt`, `loop`, `rom`, `raid1`. Text rather than an enum: the
/// set is util-linux's and grows with each stacking layer Linux gains, and a `raid` type
/// carries its level in the word.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceType(NonEmptyText);

impl DeviceType {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "device type")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&DeviceType> for Observation {
    fn from(value: &DeviceType) -> Self {
        Observation::text(value.as_str())
    }
}
