//! One container docker knows about.

use rastro_collector::Observation;

use crate::collectors::containers::model::{ContainerCommand, ContainerImage, ContainerState};
use crate::collectors::containers::value_objects::{ContainerId, EngineInstant};

/// A container as rastro means it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerContainer {
    pub id: ContainerId,
    pub created: EngineInstant,
    pub image: ContainerImage,
    pub command: ContainerCommand,
    pub state: ContainerState,
    /// Whether the engine will delete this container the moment it stops.
    pub auto_remove: bool,
}

impl From<&DockerContainer> for Observation {
    /// **A container that will delete itself is volatile whole.**
    ///
    /// `--rm` declares a job rather than a tenant. A cron-driven `docker run --rm` exists for
    /// a few seconds, so two runs of a box nobody touched legitimately disagree about whether
    /// it is there — which is precisely what `Volatility` is for, rather than something the
    /// byte-identity contract has to be weakened to accommodate.
    ///
    /// **Keyed on the engine's own record of the intent**, `HostConfig.AutoRemove`, and not
    /// guessed from a name or from how long the container has been up. The container is still
    /// reported in full in the complete view, where a reader standing in front of the box can
    /// see what ran.
    fn from(container: &DockerContainer) -> Self {
        let observed = Observation::object([
            ("auto_remove", Observation::boolean(container.auto_remove)),
            ("command", Observation::from(&container.command)),
            ("created", Observation::from(&container.created)),
            ("id", Observation::from(&container.id)),
            ("image", Observation::from(&container.image)),
            ("state", Observation::from(&container.state)),
        ]);

        match container.auto_remove {
            true => observed.volatile(),
            false => observed,
        }
    }
}
