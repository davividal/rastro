//! What the engine says a container is doing.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The engine's own word: `created`, `running`, `paused`, `restarting`, `removing`, `exited`,
/// `dead`.
///
/// **Recorded as reported, not checked against a list rastro holds.** The set belongs to the
/// engine and grows with it, nothing in this collector branches on the word, and refusing one
/// rastro had not heard of would cost the whole facet over a container it otherwise described
/// perfectly. The same reasoning as the unrecognised `pg_lsclusters` qualifier.
///
/// **Not volatile.** It changes only when the container's own life changes, and `running`
/// becoming `exited` is the single most useful line in a diff of a container host.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContainerStatus(NonEmptyText);

impl ContainerStatus {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "container status")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ContainerStatus> for Observation {
    fn from(status: &ContainerStatus) -> Self {
        Observation::text(status.as_str())
    }
}
