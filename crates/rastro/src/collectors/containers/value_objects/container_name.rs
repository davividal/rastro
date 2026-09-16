//! The name a container is known by.

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::value_objects::single_word::single_word;

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
        Ok(Self(single_word(value, "container name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
