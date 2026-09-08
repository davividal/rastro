//! One container containerd holds, one level below docker's account of the same thing.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::model::ContainerdTask;
use crate::collectors::containers::value_objects::{EngineInstant, ImageReference, LabelName};

/// A containerd container as rastro means it.
///
/// **Nowhere near docker's container, which is why the two dialects share no container
/// type.** There are no published ports here, no restart policy and no environment: those
/// are docker's abstractions above containerd, and it knows nothing of them. What containerd
/// knows and docker's account does not is the layer underneath: which OCI runtime runs it,
/// which snapshotter holds its filesystem and under which key.
///
/// On a docker box every one of these is the lower half of a container the `docker` entry
/// also describes. They are kept apart rather than merged so they can disagree: a container
/// docker has forgotten and containerd still holds is a real and awkward state, and one no
/// single merged entry could show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerdContainer {
    /// Absent for a container created from a rootfs rather than an image.
    pub image: Option<ImageReference>,
    /// The OCI runtime, `io.containerd.runc.v2` being the ordinary one.
    pub runtime: NonEmptyText,
    /// Which snapshotter holds the container's filesystem, and the key it holds it under.
    ///
    /// Both empty for a container docker manages, because docker keeps its own snapshots
    /// and hands containerd a prepared rootfs. Set for anything created through containerd
    /// itself, `nerdctl` and a kubelet included.
    pub snapshotter: Option<NonEmptyText>,
    pub snapshot_key: Option<NonEmptyText>,
    pub created: EngineInstant,
    /// When the container's own record last changed, which is not when it last ran.
    pub updated: Option<EngineInstant>,
    pub labels: BTreeMap<LabelName, String>,
    /// The sandbox this container belongs to, which on a kubernetes node is its pod.
    pub sandbox: Option<NonEmptyText>,
    /// The process it is running, absent for a container that is defined and not running.
    pub task: Option<ContainerdTask>,
}

impl From<&ContainerdContainer> for Observation {
    fn from(container: &ContainerdContainer) -> Self {
        Observation::object([
            ("created", Observation::from(&container.created)),
            (
                "image",
                match &container.image {
                    Some(image) => Observation::from(image),
                    None => Observation::null(),
                },
            ),
            (
                "labels",
                Observation::object(
                    container
                        .labels
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            ("runtime", Observation::text(container.runtime.as_str())),
            ("sandbox", optional(container.sandbox.as_ref())),
            ("snapshot_key", optional(container.snapshot_key.as_ref())),
            ("snapshotter", optional(container.snapshotter.as_ref())),
            (
                "task",
                match &container.task {
                    Some(task) => Observation::from(task),
                    None => Observation::null(),
                },
            ),
            (
                "updated",
                match &container.updated {
                    Some(updated) => Observation::from(updated),
                    None => Observation::null(),
                },
            ),
        ])
    }
}

fn optional(value: Option<&NonEmptyText>) -> Observation {
    match value {
        Some(value) => Observation::text(value.as_str()),
        None => Observation::null(),
    }
}
