//! One extension the cluster could install, and whether the answering database has.

use rastro_collector::Observation;

use crate::collectors::postgresql::value_objects::ExtensionName;

/// An entry of `pg_available_extensions`.
///
/// **Two different questions in one row.** `default_version` reads
/// `$SHAREDIR/extension/*.control` from disk, so it is cluster-wide: the same in every
/// database, and it answers "what could be installed, at what version". `installed_version` is
/// per database, and it is the version created in the *database that answered*, which the
/// cluster's `lens` names. The gap between the two (installed 1.9, default 1.11) is the
/// pending upgrade a restart or an `ALTER EXTENSION` would take.
///
/// Distinct from the per-database extension read: that lists what is actually created in each
/// database, while this lists what is installable cluster-wide. A library preloaded through
/// `shared_preload_libraries` need not have a `CREATE EXTENSION` anywhere, so the two answer
/// different questions and neither substitutes for the other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AvailableExtension {
    pub name: ExtensionName,
    /// The version the control file declares, or `None` where it omits one and a caller must
    /// name a version to install.
    pub default_version: Option<String>,

    /// The version created in the database that answered, or `None` where it is not created
    /// there.
    pub installed_version: Option<String>,
}

impl From<&AvailableExtension> for Observation {
    fn from(extension: &AvailableExtension) -> Self {
        Observation::object([
            (
                "default_version",
                match &extension.default_version {
                    Some(version) => Observation::text(version.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "installed_version",
                match &extension.installed_version {
                    Some(version) => Observation::text(version.as_str()),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
