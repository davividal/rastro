//! The identity an engine gives a container.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A container's own id, as the engine spells it.
///
/// **Constrained, and the constraint earns its keep twice.** docker and podman both mint a
/// 64-character hex id, while containerd lets whoever creates a container choose the id, so
/// requiring hex here would refuse a legal containerd container. What is required instead is
/// that an id look like an identifier at all: no whitespace, no shell metacharacters, and not
/// a leading `-`.
///
/// The second reason is the sharper one. An id read from the host is handed straight back as
/// an argument to `docker inspect`, and it is the only value in this collector that is not a
/// literal written by its author. The execution seam already guarantees no shell is involved,
/// so this is not about quoting: it is about an id that begins with `-` arriving as a *flag*
/// to the very tool being asked about it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContainerId(NonEmptyText);

/// What an id may hold beyond letters and digits: containerd's own examples use all four.
const PUNCTUATION: [char; 4] = ['.', '_', '-', ':'];

impl ContainerId {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "container id")?;

        if text.as_str().starts_with('-') {
            return Err(CollectionError::new(format!(
                "the engine reported the container id {:?}, which begins with a dash and so \
                 would reach the engine's own client as an option rather than an id",
                text.as_str()
            )));
        }

        if let Some(character) = text.as_str().chars().find(|character| {
            !character.is_ascii_alphanumeric() && !PUNCTUATION.contains(character)
        }) {
            return Err(CollectionError::new(format!(
                "the engine reported the container id {:?}, which holds {character:?} and so \
                 is not an identifier",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ContainerId> for Observation {
    fn from(id: &ContainerId) -> Self {
        Observation::text(id.as_str())
    }
}
