//! Whether an interface is up.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The interface's operational state, in the word the kernel prints.
///
/// `UP`, `DOWN`, `UNKNOWN`, and four more the kernel defines for interfaces in transition.
///
/// `UNKNOWN` is not a failure and is very common: the loopback reports it, and so does any
/// interface whose driver does not implement carrier detection. On the development box one
/// of the two virtual NICs reports `UNKNOWN` while the other reports `UP`, which is a
/// property of the virtio driver rather than of the network.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OperationalState(NonEmptyText);

impl OperationalState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "operational state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&OperationalState> for Observation {
    fn from(value: &OperationalState) -> Self {
        Observation::text(value.as_str())
    }
}
