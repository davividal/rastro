//! Whether a repository serves built packages or the sources they were built from.

use rastro_collector::{CollectionError, Observation};

/// What a repository publishes.
///
/// Exactly two, because apt's own parser accepts exactly two: `deb` and `deb-src`.
/// An unrecognised word therefore means the line was tokenised into the wrong slots,
/// not that apt grew a third kind, so it is refused rather than passed through. That
/// is the same call the kernel module state makes and the opposite of the one the
/// password hash algorithm makes, and the difference is who owns the vocabulary: a
/// fixed parser here, a growing library there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArchiveType {
    Binary,
    Source,
}

impl ArchiveType {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        match value {
            "deb" => Ok(Self::Binary),
            "deb-src" => Ok(Self::Source),
            other => Err(CollectionError::new(format!(
                "{other:?} is not an archive type apt understands"
            ))),
        }
    }

    /// The word apt spells it with, which is what an operator would grep a diff for.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Binary => "deb",
            Self::Source => "deb-src",
        }
    }
}

impl From<&ArchiveType> for Observation {
    fn from(archive_type: &ArchiveType) -> Self {
        Observation::text(archive_type.as_str())
    }
}
