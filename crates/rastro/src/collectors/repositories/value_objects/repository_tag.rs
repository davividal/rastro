//! A label apk lets a repository be pinned by.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// An apk repository tag.
///
/// apk allows `@edge https://.../edge/main`, which makes the repository available only
/// to packages asked for as `foo@edge`. It is how an Alpine box pulls one package from
/// a newer branch without moving the whole system onto it, so which tags exist is
/// state worth recording.
///
/// **apt has no equivalent, and the field is absent rather than empty there.** This is
/// the same shape the packages collector uses for dpkg's desired state, which apk
/// cannot report: one model, and the parts a system does not have are simply not
/// present.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepositoryTag(NonEmptyText);

impl RepositoryTag {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "repository tag")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&RepositoryTag> for Observation {
    fn from(tag: &RepositoryTag) -> Self {
        Observation::text(tag.as_str())
    }
}
