//! A repository system rastro found, and how to read it.

use rastro_collector::CollectionError;

use super::apk_repositories::ApkRepositories;
use super::apt_sources::AptSources;
use crate::collectors::repositories::model::RepositorySet;
use crate::collectors::repositories::value_objects::RepositorySystem;

/// One system present on the host, together with the way it has to be read.
///
/// An enum rather than a trait, the same call the packages collector makes and for the
/// same reason: the systems are read in genuinely different ways, and an exhaustive
/// match is the mechanism that makes the compiler name every site when a third arrives.
/// Adding a variant breaks [`Self::system`] and [`Self::read`] until both are answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositorySource {
    Apk(ApkRepositories),
    Apt(AptSources),
}

impl RepositorySource {
    /// The systems this host actually has.
    pub fn detect_all() -> Vec<Self> {
        RepositorySystem::ALL
            .into_iter()
            .filter_map(Self::detect)
            .collect()
    }

    /// The match is the mechanism: adding a `RepositorySystem` variant fails to compile
    /// until the new system is given something to detect.
    ///
    /// The same hazard is documented where the list lives, on `RepositorySystem::ALL`.
    fn detect(system: RepositorySystem) -> Option<Self> {
        match system {
            RepositorySystem::Apk => ApkRepositories::detect().map(Self::Apk),
            RepositorySystem::Apt => AptSources::detect().map(Self::Apt),
        }
    }

    pub fn system(&self) -> RepositorySystem {
        match self {
            Self::Apk(_) => RepositorySystem::Apk,
            Self::Apt(_) => RepositorySystem::Apt,
        }
    }

    pub fn read(&self) -> Result<RepositorySet, CollectionError> {
        match self {
            Self::Apk(repositories) => repositories.read(),
            Self::Apt(sources) => sources.read(),
        }
    }
}
