//! A container that was listed and could not be read.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

use crate::collectors::containers::value_objects::ContainerId;

/// A container the engine listed and then would not describe, with the reason it gave.
///
/// **This exists so a loss is never silent.** Reading a box's containers takes two steps, the
/// id list and then one read per container, and a `docker run --rm` from cron can end between
/// them. Dropping the container quietly would make the document claim a completeness it does
/// not have; failing the whole facet would lose every other container on the box to a race
/// that is nobody's fault.
///
/// So it is recorded, with the engine's own complaint, and the list is annotated volatile
/// where it is rendered: a container that comes and goes on its own is the host changing on
/// its own, and the diffable view is for the parts that do not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnreadableContainer {
    pub id: ContainerId,
    pub reason: NonEmptyText,
}

impl UnreadableContainer {
    pub fn new(id: ContainerId, reason: &str) -> Result<Self, CollectionError> {
        Ok(Self {
            id,
            reason: NonEmptyText::new(reason.trim(), "unreadable container reason")?,
        })
    }
}

impl From<&UnreadableContainer> for Observation {
    fn from(unreadable: &UnreadableContainer) -> Self {
        Observation::object([
            ("id", Observation::from(&unreadable.id)),
            ("reason", Observation::text(unreadable.reason.as_str())),
        ])
    }
}
