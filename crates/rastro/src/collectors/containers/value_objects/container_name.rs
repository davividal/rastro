//! The name a container is known by.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A container's name, which is what the facet keys on.
///
/// **Keyed by name rather than by id, and that is a decision about diffs.** An id is minted
/// afresh every time a container is created, so `docker compose up` on an unchanged
/// definition would report every container as removed and a new one added. A name survives
/// that: compose derives it from the project and the service, and an operator who names
/// nothing still gets a stable name until they recreate the container themselves. The id is
/// recorded as a value, where a reader can see it change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContainerName(NonEmptyText);

impl ContainerName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "container name")?;

        if text.as_str().chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "the engine reported the container name {:?}, and a name holding whitespace \
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

impl From<&ContainerName> for Observation {
    fn from(name: &ContainerName) -> Self {
        Observation::text(name.as_str())
    }
}
