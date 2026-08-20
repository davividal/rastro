//! What the hardware calls itself.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The model string the device reports.
///
/// `HARDDISK` on the development box, which is what its virtio disk claims to be. Reported
/// only for whole devices; a partition has none, because a partition is not hardware.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceModel(NonEmptyText);

impl DeviceModel {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "device model")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&DeviceModel> for Observation {
    fn from(value: &DeviceModel) -> Self {
        Observation::text(value.as_str())
    }
}
