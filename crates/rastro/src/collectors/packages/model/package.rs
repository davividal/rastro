//! One package.

use rastro_collector::Observation;

use super::installation_status::InstallationStatus;
use crate::collectors::packages::value_objects::{Architecture, PackageVersion};

/// What rastro records about a package its manager knows.
///
/// Nameless, because the name is the key it is filed under in a
/// [`PackageSet`](super::package_set::PackageSet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub version: PackageVersion,
    pub architecture: Architecture,

    /// Absent for a manager that does not track one, which today means apk.
    pub status: Option<InstallationStatus>,
}

impl From<&Package> for Observation {
    fn from(package: &Package) -> Self {
        let mut entries = vec![
            ("version", Observation::text(package.version.as_str())),
            (
                "architecture",
                Observation::text(package.architecture.as_str()),
            ),
        ];

        // Omitted rather than null: a key that is absent says "this manager does not
        // report one", where a null would read as "reported, and empty".
        if let Some(status) = &package.status {
            entries.push(("status", Observation::from(status)));
        }

        Observation::object(entries)
    }
}
