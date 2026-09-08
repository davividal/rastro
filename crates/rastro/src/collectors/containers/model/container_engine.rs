//! One engine, whichever flavour it is.

use rastro_collector::Observation;

use crate::collectors::containers::model::{ContainerdEngine, DockerEngine};
use crate::collectors::containers::value_objects::EngineFlavour;

/// An engine rastro found, in the shape its own concepts have.
///
/// **Not one model with a field per engine's worth of optional detail.** The engines do not
/// describe the same thing at the same level: containerd knows nothing of published ports or
/// a restart policy, because those are docker's abstractions above it, and a shared container
/// type would either lie by omission or collect an optional field for every concept any one
/// engine has. What they genuinely share is identity, and that lives in the value objects
/// both use.
///
/// An enum rather than a trait for the reason the packages facet gives: an exhaustive match
/// is the mechanism that makes the compiler name every site when a third engine arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerEngine {
    Containerd(ContainerdEngine),
    /// Boxed because the two dialects are nowhere near the same size: docker's entry carries
    /// every container, image, volume and network on the box, and without the indirection
    /// every value of this enum would be as large as the largest of them.
    Docker(Box<DockerEngine>),
}

impl ContainerEngine {
    pub fn flavour(&self) -> EngineFlavour {
        match self {
            Self::Containerd(_) => EngineFlavour::Containerd,
            Self::Docker(_) => EngineFlavour::Docker,
        }
    }
}

impl From<&ContainerEngine> for Observation {
    fn from(engine: &ContainerEngine) -> Self {
        match engine {
            ContainerEngine::Containerd(containerd) => Observation::from(containerd),
            ContainerEngine::Docker(docker) => Observation::from(docker.as_ref()),
        }
    }
}
