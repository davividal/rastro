//! The commit an agent was built from.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The git revision an agent prints beside its version: `6876475a`.
///
/// **Recorded separately because it is the half that moves on its own.** Two binaries can
/// both call themselves `1.12.1` and differ, which is exactly what happens when an internal
/// pipeline rebuilds a release. A facet carrying only the version would report no change at
/// all across that swap, and these five agents are installed by dropping a binary on the
/// box, which is the deployment style where that happens most.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildRevision(NonEmptyText);

impl BuildRevision {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "build revision")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&BuildRevision> for Observation {
    fn from(revision: &BuildRevision) -> Self {
        Observation::text(revision.as_str())
    }
}
