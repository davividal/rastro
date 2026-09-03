//! The version of the binary that would serve.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// What nginx calls itself after the slash in its banner: `1.30.4`.
///
/// Not parsed into numbers. A version is compared with the same binary's version on another
/// run, where equal or not equal is the whole question, and the forks spell theirs their own
/// way: `openresty/1.21.4.1` has four parts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildVersion(NonEmptyText);

impl BuildVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "nginx version")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&BuildVersion> for Observation {
    fn from(version: &BuildVersion) -> Self {
        Observation::text(version.as_str())
    }
}
