//! What a docker daemon says it is running with.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::containers::model::{
    CgroupControl, DockerContainers, DockerImages, DockerNetworks, DockerVolumes,
};
use crate::collectors::containers::value_objects::{EngineVersion, StorageDriver, SwarmState};

/// The daemon's own account of itself, which only exists when a daemon answered.
///
/// **A type of its own rather than a handful of optional fields on the engine**, and that is
/// the point: a box whose daemon is not answering has no server node at all, so a reader
/// cannot mistake "rastro could not ask" for "the daemon answered and reported nothing". The
/// postgresql facet learned the same lesson keeping a cluster's configured half apart from
/// its observed one.
///
/// **Counts are deliberately absent.** `docker info` reports how many containers and images
/// there are, and this facet reports the containers and the images themselves, so a count
/// would be a second, worse account of the same fact, and one that changes whenever a
/// short-lived container comes and goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerServer {
    pub version: EngineVersion,
    /// Where the daemon keeps its layers, images and container state.
    ///
    /// Optional because it is also what the filesystem claim is resolved from, and a claim
    /// rastro cannot resolve is better left unmade than made against a distribution default
    /// the box may not use.
    pub root_directory: Option<AbsolutePath>,
    pub storage_driver: StorageDriver,
    pub cgroup: CgroupControl,
    /// The driver a container gets when it asks for none, which decides where the logs of
    /// every such container go.
    pub logging_driver: NonEmptyText,
    pub default_runtime: NonEmptyText,
    /// Whether containers survive the daemon restarting, which is why a daemon upgrade is or
    /// is not an outage.
    pub live_restore: bool,
    pub swarm: SwarmState,
    /// The confinement the daemon applies by default: seccomp, apparmor, selinux, userns,
    /// rootless, cgroupns.
    ///
    /// **Sorted, because docker promises nothing about the order.** An order that moved
    /// between two runs of an unchanged box would break byte-identity for no observed change.
    /// This is also where rootlessness shows: docker 29 has no `Rootless` field of its own
    /// and reports `name=rootless` here instead, which was measured rather than remembered.
    pub security_options: Vec<NonEmptyText>,
    /// Which containerd, runc and init the engine actually runs, keyed by component name.
    ///
    /// Free from the same probe that establishes the daemon is answering, and worth keeping:
    /// a runc replaced under a running docker is exactly the change a fingerprint is taken
    /// around, and it is invisible in the engine's own version.
    pub components: BTreeMap<NonEmptyText, EngineVersion>,
    /// What is on the box, and what could not be read while looking.
    ///
    /// Held by the server rather than by the engine, for the same reason the rest of this
    /// type is: a daemon that did not answer has no container list, and there is a difference
    /// between an empty list and no list at all.
    pub containers: DockerContainers,
    /// What the engine holds, whether or not anything is running it.
    pub images: DockerImages,
    /// The volumes, which outlive the containers that used them.
    pub volumes: DockerVolumes,
    /// The networks, the engine's own three included.
    pub networks: DockerNetworks,
}

impl From<&DockerServer> for Observation {
    fn from(server: &DockerServer) -> Self {
        Observation::object([
            ("cgroup", Observation::from(&server.cgroup)),
            (
                "containers",
                Observation::object(
                    server
                        .containers
                        .named()
                        .iter()
                        .map(|(name, container)| (name.as_str(), Observation::from(container))),
                ),
            ),
            (
                "components",
                Observation::object(
                    server
                        .components
                        .iter()
                        .map(|(name, version)| (name.as_str(), Observation::from(version))),
                ),
            ),
            ("images", Observation::from(&server.images)),
            (
                "default_runtime",
                Observation::text(server.default_runtime.as_str()),
            ),
            ("live_restore", Observation::boolean(server.live_restore)),
            (
                "logging_driver",
                Observation::text(server.logging_driver.as_str()),
            ),
            (
                "root_directory",
                match &server.root_directory {
                    Some(directory) => Observation::text(directory.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "security_options",
                Observation::list(
                    server
                        .security_options
                        .iter()
                        .map(|option| Observation::text(option.as_str())),
                ),
            ),
            ("storage_driver", Observation::from(&server.storage_driver)),
            ("swarm", Observation::from(&server.swarm)),
            ("networks", Observation::from(&server.networks)),
            (
                "unreadable_networks",
                Observation::list(server.networks.unreadable().iter().map(Observation::from))
                    .volatile(),
            ),
            (
                "unreadable_volumes",
                Observation::list(server.volumes.unreadable().iter().map(Observation::from))
                    .volatile(),
            ),
            ("volumes", Observation::from(&server.volumes)),
            (
                "unreadable_images",
                Observation::list(server.images.unreadable().iter().map(Observation::from))
                    .volatile(),
            ),
            (
                // Volatile, because a container that came and went between the id list and
                // the read of it is the host changing on its own. See `UnreadableObject`.
                "unreadable_containers",
                Observation::list(server.containers.unreadable().iter().map(Observation::from))
                    .volatile(),
            ),
            ("version", Observation::from(&server.version)),
        ])
    }
}
