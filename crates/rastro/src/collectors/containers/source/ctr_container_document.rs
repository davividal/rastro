//! `ctr containers info`: containerd's spelling of one container.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::model::{ContainerdContainer, ContainerdTask};
use crate::collectors::containers::value_objects::{
    ContainerId, EngineInstant, ImageReference, LabelName,
};

/// A container as containerd describes it, kept apart from rastro's meaning.
///
/// **`Spec` is deliberately not declared, and it is thirty of the thirty-three kilobytes
/// this command prints.** It is the OCI runtime spec: the container's mounts, capabilities,
/// namespaces and resource limits. On a docker box every one of those is docker's account to
/// give, and the `docker` entry gives it in rastro's own vocabulary rather than the spec's.
/// serde ignores what is not asked for, so not asking is how the document stays small.
#[derive(Debug, Clone, Deserialize)]
pub struct CtrContainerDocument {
    #[serde(rename = "ID")]
    id: String,
    /// Null on a container with none, which `default` covers either way.
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
    /// Empty for a container created from a rootfs rather than an image.
    #[serde(rename = "Image", default)]
    image: String,
    #[serde(rename = "Runtime")]
    runtime: RuntimeHalf,
    /// Both empty for a container docker manages, which keeps its own snapshots.
    #[serde(rename = "SnapshotKey", default)]
    snapshot_key: String,
    #[serde(rename = "Snapshotter", default)]
    snapshotter: String,
    #[serde(rename = "CreatedAt")]
    created: String,
    #[serde(rename = "UpdatedAt", default)]
    updated: String,
    /// The pod, on a kubernetes node.
    #[serde(rename = "SandboxID", default)]
    sandbox: String,
}

/// Only the runtime's name is read: its `Options` are a base64 protobuf blob whose contents
/// are the runtime's own business, and recording the encoding would put bytes in the
/// document nobody can read.
#[derive(Debug, Clone, Deserialize)]
struct RuntimeHalf {
    #[serde(rename = "Name")]
    name: String,
}

impl CtrContainerDocument {
    /// Translates containerd's document into rastro's model, with the task it is running.
    pub fn to_container(
        &self,
        task: Option<ContainerdTask>,
    ) -> Result<(ContainerId, ContainerdContainer), CollectionError> {
        let mut labels = BTreeMap::new();
        for (name, value) in self.labels.iter().flatten() {
            labels.insert(LabelName::new(name.clone())?, value.clone());
        }

        let container = ContainerdContainer {
            image: ImageReference::new(self.image.clone()).ok(),
            runtime: NonEmptyText::new(self.runtime.name.clone(), "container runtime")?,
            snapshotter: NonEmptyText::new(self.snapshotter.clone(), "snapshotter").ok(),
            snapshot_key: NonEmptyText::new(self.snapshot_key.clone(), "snapshot key").ok(),
            created: EngineInstant::new(self.created.clone())?,
            updated: EngineInstant::new(self.updated.clone()).ok(),
            labels,
            sandbox: NonEmptyText::new(self.sandbox.clone(), "sandbox id").ok(),
            task,
        };

        Ok((ContainerId::new(self.id.clone())?, container))
    }
}
