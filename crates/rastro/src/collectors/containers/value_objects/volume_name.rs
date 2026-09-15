//! The name a volume is known by.

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::value_objects::single_word::single_word;

/// A volume's name, which is its identity to the engine and to every container that mounts
/// it.
///
/// **Unlike a container, a volume's name is never minted afresh**: it is what an operator
/// or a compose file chose, and it survives every container that used it. An anonymous
/// volume, which a container gets when an image declares a `VOLUME` and nobody named one,
/// carries a 64-character hex name instead and is recorded like any other, since it holds
/// disk and outlives the container that caused it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VolumeName(NonEmptyText);

impl VolumeName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(single_word(value, "volume name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
