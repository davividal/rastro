//! What sort of thing is mounted into a container.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// docker's own word: `volume`, `bind`, `tmpfs`, `npipe`, `cluster`.
///
/// **Recorded as reported rather than checked against a list rastro holds**, for the reason
/// the container status is: the set belongs to the engine, nothing here branches on the
/// word, and refusing one rastro had not heard of would cost the whole facet over a mount it
/// otherwise described perfectly.
///
/// A bind and a volume at the same destination are very different facts, which is why this is
/// recorded at all: a bind reaches out of the container onto the host's own tree, and that is
/// the mount an operator needs to see appear.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MountKind(NonEmptyText);

/// What a tmpfs mount is, since docker reports those outside the mount list and so never
/// spells the word itself.
const TMPFS: &str = "tmpfs";

impl MountKind {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "mount kind")?))
    }

    /// The kind a `HostConfig.Tmpfs` entry is, named here so the source does not spell it.
    pub fn tmpfs() -> Self {
        Self::new(TMPFS).expect("`tmpfs` is a legal mount kind")
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&MountKind> for Observation {
    fn from(kind: &MountKind) -> Self {
        Observation::text(kind.as_str())
    }
}
