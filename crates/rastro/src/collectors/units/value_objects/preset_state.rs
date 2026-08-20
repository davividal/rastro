//! What the distribution's preset policy expects.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// What the distribution's preset policy says a unit *should* be.
///
/// `enabled` or `disabled`, and absent for a unit no preset covers, which is most of
/// them: 189 of the 262 unit files on the development box have none.
///
/// Worth recording alongside the actual state because the pair is what says whether a
/// box has been deliberately configured. A unit that is `disabled` with a preset of
/// `enabled` was switched off by somebody; one that is `disabled` with no preset is
/// merely untouched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PresetState(NonEmptyText);

impl PresetState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "preset state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&PresetState> for Observation {
    fn from(value: &PresetState) -> Self {
        Observation::text(value.as_str())
    }
}
