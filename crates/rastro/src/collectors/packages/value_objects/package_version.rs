//! Which version of a package is on the host.

use rastro_collector::{CollectionError, NonEmptyText};

/// A version string, as its manager spelled it.
///
/// Not parsed into components. rastro diffs versions and never compares them, and
/// Debian and apk order versions by different grammars, so a shared parse would either
/// be wrong for one of them or duplicate two upstream implementations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageVersion(NonEmptyText);

impl PackageVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "package version")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
