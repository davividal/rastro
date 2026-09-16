//! One container a podman service reports.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::model::PodmanPortBinding;
use crate::collectors::containers::value_objects::{
    ContainerId, ContainerStatus, ExposedPort, ImageDigest, ImageReference, LabelName,
};

/// A podman container as rastro means it.
///
/// **Timestamps are whole seconds since the epoch, which is podman's own unit here.** docker
/// reports RFC 3339 text and podman's `ps` reports integers; each dialect records what its
/// engine said rather than converting into the other's spelling, and the key names carry the
/// unit the way the filesystem facet's do.
///
/// **Pods are podman's own concept and have no docker equivalent.** A pod is a group of
/// containers sharing namespaces, and its infra container is the one holding them open, so
/// both are recorded: a container that belongs to a pod is reachable in ways a standalone
/// one is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodmanContainer {
    pub id: ContainerId,
    pub image: ImageReference,
    /// podman prints this as bare hex where docker prints `sha256:…`, and each is recorded
    /// as its engine spells it.
    pub image_id: ImageDigest,
    pub state: ContainerStatus,
    pub exit_code: i64,
    pub created_seconds_since_epoch: i64,
    /// Absent for a container that has never run.
    pub started_seconds_since_epoch: Option<i64>,
    /// Absent for a container that has not stopped, which podman reports as Go's zero time
    /// in seconds, `-62135596800`, rather than as a missing field.
    pub exited_seconds_since_epoch: Option<i64>,
    pub restarts: i64,
    /// The pod this container belongs to, absent for a standalone one.
    pub pod: Option<NonEmptyText>,
    /// Whether this is a pod's infra container, the one holding the shared namespaces open.
    pub is_infra: bool,
    pub auto_remove: bool,
    pub labels: BTreeMap<LabelName, String>,
    /// The networks it is attached to, sorted.
    pub networks: Vec<NonEmptyText>,
    pub ports: BTreeMap<ExposedPort, Vec<PodmanPortBinding>>,
}

impl From<&PodmanContainer> for Observation {
    /// A container that will delete itself is volatile whole, on the rule docker's are.
    fn from(container: &PodmanContainer) -> Self {
        let observed = Observation::object([
            ("auto_remove", Observation::boolean(container.auto_remove)),
            (
                "created_seconds_since_epoch",
                Observation::integer(container.created_seconds_since_epoch),
            ),
            ("exit_code", Observation::integer(container.exit_code)),
            (
                "exited_seconds_since_epoch",
                stamp(container.exited_seconds_since_epoch),
            ),
            ("id", Observation::from(&container.id)),
            ("image", Observation::from(&container.image)),
            ("image_id", Observation::from(&container.image_id)),
            ("is_infra", Observation::boolean(container.is_infra)),
            (
                "labels",
                Observation::object(
                    container
                        .labels
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            (
                "networks",
                Observation::list(
                    container
                        .networks
                        .iter()
                        .map(|network| Observation::text(network.as_str())),
                ),
            ),
            (
                "pod",
                match &container.pod {
                    Some(pod) => Observation::text(pod.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "ports",
                Observation::object(container.ports.iter().map(|(port, bindings)| {
                    (
                        port.as_key(),
                        Observation::list(bindings.iter().map(Observation::from)),
                    )
                })),
            ),
            (
                "restarts",
                Observation::integer(container.restarts).volatile(),
            ),
            (
                "started_seconds_since_epoch",
                stamp(container.started_seconds_since_epoch),
            ),
            ("state", Observation::from(&container.state)),
        ]);

        match container.auto_remove {
            true => observed.volatile(),
            false => observed,
        }
    }
}

/// A stamp that moves on its own, or absent where the engine reported none.
fn stamp(seconds: Option<i64>) -> Observation {
    match seconds {
        Some(seconds) => Observation::integer(seconds).volatile(),
        None => Observation::null().volatile(),
    }
}
