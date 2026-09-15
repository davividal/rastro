//! The release an engine reports itself as.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A version exactly as the engine prints it: `29.8.0`, `v2.3.4`.
///
/// **Never normalised**, for the reason the exporters facet reached first: docker prints its
/// own version without a `v` and the containerd it embeds with one, and editing either to
/// make the pair match would be rastro reshaping what a host reported.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EngineVersion(NonEmptyText);

impl EngineVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "container engine version")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&EngineVersion> for Observation {
    fn from(version: &EngineVersion) -> Self {
        Observation::text(version.as_str())
    }
}
