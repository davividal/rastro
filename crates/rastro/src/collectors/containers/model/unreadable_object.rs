//! Something the engine listed and would not describe.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// One of the engine's objects that was listed and then would not be described, with the
/// reason it gave.
///
/// **This exists so a loss is never silent.** Reading a box's containers takes two steps,
/// the id list and then one read per container, and a `docker run --rm` from cron can end
/// between them. Dropping the container quietly would make the document claim a
/// completeness it does not have; failing the whole facet would lose every other container
/// on the box to a race that is nobody's fault.
///
/// **Object, because that is docker's own word for the four kinds of thing it holds**, and
/// because the same race applies to all four: `docker build` and `docker image prune` do to
/// images exactly what a cron `--rm` does to containers. One type rather than four, so the
/// facet has one strategy for the concern rather than a variant per kind.
///
/// The id arrives already validated by whichever kind it belongs to — a container id, an
/// image digest — so it is held as text here rather than re-checked against a rule that
/// would have to be the union of all four.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnreadableObject {
    pub id: NonEmptyText,
    pub reason: NonEmptyText,
}

impl UnreadableObject {
    pub fn new(id: &str, reason: &str) -> Result<Self, CollectionError> {
        Ok(Self {
            id: NonEmptyText::new(id, "unreadable object id")?,
            reason: NonEmptyText::new(reason.trim(), "unreadable object reason")?,
        })
    }
}

impl From<&UnreadableObject> for Observation {
    fn from(unreadable: &UnreadableObject) -> Self {
        Observation::object([
            ("id", Observation::text(unreadable.id.as_str())),
            ("reason", Observation::text(unreadable.reason.as_str())),
        ])
    }
}
