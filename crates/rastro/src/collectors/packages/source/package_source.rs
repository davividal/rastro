//! A package manager rastro found, and how to read it.

use rastro_collector::CollectionError;

use super::apk_database::ApkDatabase;
use super::dpkg_query::DpkgQuery;
use crate::collectors::packages::model::PackageSet;
use crate::collectors::packages::value_objects::PackageManager;

/// One manager present on the host, together with the way it has to be read.
///
/// An enum rather than a trait, because the managers are read in genuinely different ways
/// and an exhaustive match is the mechanism that makes the compiler name every site when a
/// third arrives. Adding a variant breaks [`Self::manager`] and [`Self::read`] until both
/// are answered.
///
/// The collector holds a list of these rather than one `Option` field per manager, so that
/// adding a manager touches no hand-written pairing. The original reason was stronger and no
/// longer applies: `presence` used to consult the sources, so forgetting to extend it would
/// have made a box that had a manager report `absent`. It is now unconditionally `Present`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageSource {
    Apk(ApkDatabase),
    Dpkg(DpkgQuery),
}

impl PackageSource {
    /// The managers this host actually has.
    pub fn detect_all() -> Vec<Self> {
        PackageManager::ALL
            .into_iter()
            .filter_map(Self::detect)
            .collect()
    }

    /// The match is the mechanism: adding a `PackageManager` variant fails to compile until the
    /// new manager is given something to detect.
    ///
    /// The same hazard is documented where the list lives, on `PackageManager::ALL`.
    fn detect(manager: PackageManager) -> Option<Self> {
        match manager {
            PackageManager::Apk => ApkDatabase::detect().map(Self::Apk),
            PackageManager::Dpkg => DpkgQuery::detect().map(Self::Dpkg),
        }
    }

    pub fn manager(&self) -> PackageManager {
        match self {
            Self::Apk(_) => PackageManager::Apk,
            Self::Dpkg(_) => PackageManager::Dpkg,
        }
    }

    pub fn read(&self) -> Result<PackageSet, CollectionError> {
        match self {
            Self::Apk(database) => database.read(),
            Self::Dpkg(query) => query.read(),
        }
    }
}
