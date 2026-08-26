//! The release an agent reports itself as.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A version exactly as the binary prints it: `1.12.1`, `v0.49.2`.
///
/// **The leading `v` is kept or absent as the agent spells it, never normalised.** cAdvisor
/// prints `v0.49.2` and node_exporter prints `1.12.1`, and stripping the `v` to make them
/// match would be rastro editing what a host reported to fit a shape rastro preferred.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExporterVersion(NonEmptyText);

impl ExporterVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "exporter version")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ExporterVersion> for Observation {
    fn from(version: &ExporterVersion) -> Self {
        Observation::text(version.as_str())
    }
}
