//! Where a repository is served from.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A repository's location, as the configuration spells it.
///
/// **Text rather than a parsed URL, and deliberately not validated as one.** The
/// values that legitimately appear here are not all URLs: `mirror+file:///etc/apt/
/// mirrors/debian.list` names a *file listing mirrors*, `cdrom:[Debian ...]/` names a
/// disc, `copy:/srv/local` and a bare `/srv/local` name the filesystem, and apk
/// accepts a plain directory path. Insisting on a scheme would refuse half of them,
/// and rewriting them into some canonical form would record a location the file does
/// not contain.
///
/// **The indirection is recorded, not resolved.** A `mirror+file:` entry is kept as
/// written, so what the fingerprint says is "this box takes Debian from whichever
/// mirrors that file lists", which is the configured state. Following it would mean
/// reading a second file whose contents are chosen by apt's mirror-selection logic at
/// download time, and that answer is not a property of this box's configuration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepositoryUri(NonEmptyText);

impl RepositoryUri {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "repository uri")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&RepositoryUri> for Observation {
    fn from(uri: &RepositoryUri) -> Self {
        Observation::text(uri.as_str())
    }
}
