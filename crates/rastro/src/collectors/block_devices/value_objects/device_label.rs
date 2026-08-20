//! A name somebody gave a device.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A filesystem or partition label.
///
/// Free text an operator chose, so it says what a device is *for* in a way no other field
/// does. Absent far more often than present.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceLabel(NonEmptyText);

impl DeviceLabel {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "device label")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&DeviceLabel> for Observation {
    fn from(value: &DeviceLabel) -> Self {
        Observation::text(value.as_str())
    }
}
