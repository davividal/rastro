//! Whether this box is part of a swarm.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// docker's own word for the node's swarm membership: `inactive`, `pending`, `active`,
/// `locked`, `error`.
///
/// **Recorded as the engine spells it, never checked against a list rastro holds.** The set
/// belongs to docker and can grow, and a membership rastro has not heard of is still a fact
/// about the box, while refusing it would cost the whole facet. The same reasoning as the
/// unrecognised `pg_lsclusters` qualifier in the postgresql facet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SwarmState(NonEmptyText);

impl SwarmState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "swarm membership")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SwarmState> for Observation {
    fn from(state: &SwarmState) -> Self {
        Observation::text(state.as_str())
    }
}
