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
    /// Every manager rastro knows how to read, keeping the ones this host has.
    ///
    /// Forgetting to add a manager here is a gap in coverage rather than a false report:
    /// the facet still describes exactly what was found, and rastro never claims to know
    /// every manager in existence.
    pub fn detect_all() -> Vec<Self> {
        [
            ApkDatabase::detect().map(Self::Apk),
            DpkgQuery::detect().map(Self::Dpkg),
        ]
        .into_iter()
        .flatten()
        .collect()
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
