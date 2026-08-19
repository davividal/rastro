//! Which manager reported a set of packages.

use rastro_collector::Observation;

/// A package manager rastro can read.
///
/// The facet is keyed by this rather than merging every manager into one list, so a box
/// carrying two needs no arbitrary precedence and the two shapes may differ honestly:
/// dpkg reports a desired state and packages that are *not* installed, apk does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackageManager {
    Apk,
    Dpkg,
}

impl PackageManager {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apk => "apk",
            Self::Dpkg => "dpkg",
        }
    }
}

impl From<&PackageManager> for Observation {
    fn from(manager: &PackageManager) -> Self {
        Observation::text(manager.as_str())
    }
}
