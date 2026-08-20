//! The unit-type-specific detail behind a unit's active state.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The unit-type-specific detail behind the active state.
///
/// Its vocabulary depends on the unit type and is by far the widest here: `running`,
/// `exited` and `dead` for services, `plugged` for devices, `mounted` for mounts,
/// `listening` for sockets, `waiting` for timers and paths. systemd documents no closed
/// list, which settles the text-rather-than-enum question on its own.
///
/// The pairing with the active state is what carries meaning: `active/exited` is a
/// oneshot service that finished successfully, and `active/running` is a daemon still
/// there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubState(NonEmptyText);

impl SubState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "sub state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SubState> for Observation {
    fn from(value: &SubState) -> Self {
        Observation::text(value.as_str())
    }
}
