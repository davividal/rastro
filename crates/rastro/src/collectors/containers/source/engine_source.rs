//! An engine rastro found, and the way it has to be read.

use rastro_collector::CollectionError;

use super::docker::Docker;
use crate::collectors::containers::model::ContainerEngine;
use crate::collectors::containers::value_objects::EngineFlavour;

/// One engine present on the host, together with its own interface.
///
/// An enum rather than a trait, for the reason the packages facet gives: the engines are read
/// in genuinely different ways, and an exhaustive match is what makes the compiler name every
/// site when a third arrives. Adding an [`EngineFlavour`] variant fails to compile until
/// [`Self::detect`] says what to look for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineSource {
    Docker(Docker),
}

impl EngineSource {
    /// The engines this host actually has.
    pub fn detect_all() -> Vec<Self> {
        EngineFlavour::ALL
            .into_iter()
            .filter_map(Self::detect)
            .collect()
    }

    fn detect(flavour: EngineFlavour) -> Option<Self> {
        match flavour {
            EngineFlavour::Docker => Docker::detect().map(Self::Docker),
        }
    }

    pub fn read(&self) -> Result<ContainerEngine, CollectionError> {
        match self {
            Self::Docker(docker) => Ok(ContainerEngine::Docker(docker.read()?)),
        }
    }
}
