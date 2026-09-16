//! One container docker knows about.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::containers::model::{
    ContainerCommand, ContainerDevice, ContainerEnvironment, ContainerHealthcheck, ContainerImage,
    ContainerLabels, ContainerLimits, ContainerLogging, ContainerMounts, ContainerNetworks,
    ContainerPorts, ContainerSecurity, ContainerState, NameResolution, ResourceLimit,
    RestartPolicy,
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
    pub mounts: ContainerMounts,
    pub networks: ContainerNetworks,
    pub restart_policy: RestartPolicy,
    pub limits: ContainerLimits,
    pub security: ContainerSecurity,
    /// Absent for a container with no check configured.
    pub healthcheck: Option<ContainerHealthcheck>,
    pub logging: ContainerLogging,
    /// The host devices it was given, keyed by where each appears inside it.
    pub devices: BTreeMap<AbsolutePath, ContainerDevice>,
    /// Its ulimits, keyed by name.
    pub ulimits: BTreeMap<NonEmptyText, ResourceLimit>,
    /// The kernel parameters it asked for, keyed by name.
    ///
    /// **Its own, not the host's.** The `sysctl` facet reports what the running kernel is
    /// set to; this is a request in a container's definition, and the two are different
    /// facts about different things.
    pub kernel_parameters: BTreeMap<NonEmptyText, String>,
    pub name_resolution: NameResolution,
    pub ports: ContainerPorts,
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
            (
                "devices",
                Observation::object(
                    container
                        .devices
                        .iter()
                        .map(|(inside, device)| (inside.as_str(), Observation::from(device))),
                ),
            ),
            ("environment", Observation::from(&container.environment)),
            (
                "healthcheck",
                match &container.healthcheck {
                    Some(healthcheck) => Observation::from(healthcheck),
                    None => Observation::null(),
                },
            ),
            ("id", Observation::from(&container.id)),
            ("image", Observation::from(&container.image)),
            (
                "kernel_parameters",
                Observation::object(
                    container
                        .kernel_parameters
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            ("labels", Observation::from(&container.labels)),
            ("limits", Observation::from(&container.limits)),
            ("logging", Observation::from(&container.logging)),
            ("mounts", Observation::from(&container.mounts)),
            (
                "name_resolution",
                Observation::from(&container.name_resolution),
            ),
            ("networks", Observation::from(&container.networks)),
            ("ports", Observation::from(&container.ports)),
            (
                "restart_policy",
                Observation::from(&container.restart_policy),
            ),
            ("security", Observation::from(&container.security)),
            (
                "ulimits",
                Observation::object(
                    container
                        .ulimits
                        .iter()
                        .map(|(name, limit)| (name.as_str(), Observation::from(limit))),
                ),
            ),
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
