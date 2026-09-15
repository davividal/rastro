//! An engine rastro found, and the way it has to be read.

use rastro_collector::{CollectionError, WalkedTree};

use super::containerd::Containerd;
use super::docker::Docker;
use super::podman::Podman;
use crate::collectors::containers::model::ContainerEngine;
use crate::collectors::containers::value_objects::{EngineFlavour, EngineInstance};

/// One engine present on the host, together with its own interface.
///
/// An enum rather than a trait, for the reason the packages facet gives: the engines are read
/// in genuinely different ways, and an exhaustive match is what makes the compiler name every
/// site when a third arrives. Adding an [`EngineFlavour`] variant fails to compile until the
/// detection below says what to look for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineSource {
    Containerd(Containerd),
    Docker(Docker),
    Podman(Podman),
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
            EngineFlavour::Containerd => Containerd::detect().map(Self::Containerd),
            EngineFlavour::Docker => Docker::detect().map(Self::Docker),
            EngineFlavour::Podman => Podman::detect().map(Self::Podman),
        }
    }

    /// The account this engine belongs to.
    ///
    /// Every engine rastro reads today is root's: dockerd and containerd are system
    /// services, and a rootful podman service is root's too. A rootless podman belongs to
    /// the user running it, which is what this will answer once they are discovered.
    pub fn instance(&self) -> EngineInstance {
        match self {
            Self::Containerd(_) | Self::Docker(_) => EngineInstance::root(),
            Self::Podman(podman) => podman.instance(),
        }
    }

    pub fn read(&self) -> Result<ContainerEngine, CollectionError> {
        match self {
            Self::Containerd(containerd) => Ok(ContainerEngine::Containerd(containerd.read()?)),
            Self::Docker(docker) => Ok(ContainerEngine::Docker(Box::new(docker.read()?))),
            Self::Podman(podman) => Ok(ContainerEngine::Podman(podman.read()?)),
        }
    }

    /// The trees this engine keeps to itself.
    pub fn private_trees(&self) -> Vec<WalkedTree> {
        match self {
            Self::Containerd(containerd) => containerd.private_trees(),
            Self::Docker(docker) => docker.private_trees(),
            Self::Podman(podman) => podman.private_trees(),
        }
    }
}
