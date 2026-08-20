//! A device's serial number.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The serial number the device reports.
///
/// Absent on the development box, where the virtual disk has none, and present on real
/// hardware. It identifies the physical device rather than its contents, so a disk swapped
/// for an identical model shows up here and in no other field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SerialNumber(NonEmptyText);

impl SerialNumber {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "serial number")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SerialNumber> for Observation {
    fn from(value: &SerialNumber) -> Self {
        Observation::text(value.as_str())
    }
}
