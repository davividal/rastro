//! The name a volume is known by.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

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
        let text = NonEmptyText::new(value, "volume name")?;

        if text.as_str().chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "the engine reported the volume name {:?}, and a name holding whitespace \
                 means the answer was misread",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&VolumeName> for Observation {
    fn from(name: &VolumeName) -> Self {
        Observation::text(name.as_str())
    }
}
