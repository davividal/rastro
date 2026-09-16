//! The image a container was asked to run.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A reference as the operator wrote it: `alpine`, `nginx:1.29`, `ghcr.io/org/app@sha256:…`.
///
/// **Kept exactly as given, never expanded.** docker resolves a bare `alpine` to
/// `docker.io/library/alpine:latest`, and writing that into the document would replace what
/// the operator declared with rastro's reading of it. The reference and the digest it
/// resolved to are recorded side by side instead, which is the pair that makes a repointed
/// tag visible.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImageReference(NonEmptyText);

impl ImageReference {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "image reference")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ImageReference> for Observation {
    fn from(reference: &ImageReference) -> Self {
        Observation::text(reference.as_str())
    }
}
