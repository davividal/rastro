//! One container docker knows about.

use rastro_collector::{AbsolutePath, Observation};

use crate::collectors::containers::model::{
    ContainerCommand, ContainerEnvironment, ContainerImage, ContainerLabels, ContainerState,
};
use crate::collectors::containers::value_objects::{ContainerAccount, ContainerId, EngineInstant};

/// A container as rastro means it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerContainer {
    pub id: ContainerId,
    pub created: EngineInstant,
    pub image: ContainerImage,
    pub command: ContainerCommand,
    pub state: ContainerState,
    /// Absent where the image decides, which docker reports as an empty string.
    pub user: Option<ContainerAccount>,
    /// Absent where the image decides, for the same reason.
    pub working_directory: Option<AbsolutePath>,
    pub environment: ContainerEnvironment,
    pub labels: ContainerLabels,
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
            ("environment", Observation::from(&container.environment)),
            ("id", Observation::from(&container.id)),
            ("image", Observation::from(&container.image)),
            ("labels", Observation::from(&container.labels)),
            ("state", Observation::from(&container.state)),
            (
                "user",
                match &container.user {
                    Some(account) => Observation::from(account),
                    None => Observation::null(),
                },
            ),
            (
                "working_directory",
                match &container.working_directory {
                    Some(directory) => Observation::text(directory.as_str()),
                    None => Observation::null(),
                },
            ),
        ]);

        match container.auto_remove {
            true => observed.volatile(),
            false => observed,
        }
    }
}
