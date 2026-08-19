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
/// This is also why the collector holds a list of these rather than one `Option` field per
/// manager. With a field each, adding a manager and forgetting to extend `presence` would
/// make a box that has it report `absent`, which is exactly the confident lie the
/// three-valued `Presence` exists to prevent, and nothing would fail to compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageSource {
    Apk(ApkDatabase),
    Dpkg(DpkgQuery),
}

impl PackageSource {
    /// Every manager rastro knows how to read.
    ///
    /// One list, used both to detect and to say what was looked for when nothing is found,
    /// so the two cannot disagree.
    pub fn known_managers() -> [PackageManager; 2] {
        [PackageManager::Apk, PackageManager::Dpkg]
    }

    /// The managers this host actually has.
    pub fn detect_all() -> Vec<Self> {
        Self::known_managers()
            .into_iter()
            .filter_map(Self::detect)
            .collect()
    }

    /// The match is the mechanism: adding a `PackageManager` variant fails to compile until
    /// the new manager is given something to detect.
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
