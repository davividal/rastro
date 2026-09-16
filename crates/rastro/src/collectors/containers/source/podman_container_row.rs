//! `podman --remote ps --format json`: podman's spelling of one container.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::model::{PodmanContainer, PodmanPortBinding};
use crate::collectors::containers::value_objects::{
    ContainerId, ContainerName, ContainerStatus, ExposedPort, ImageDigest, ImageReference,
    LabelName, TransportProtocol,
};
use crate::collectors::inet::{InetHost, PortNumber};

/// Go's zero time in whole seconds, which is what podman prints for a stamp that has not
/// happened.
///
/// **Measured**: a running container reports `"ExitedAt": -62135596800`. It is the same trap
/// docker sets with `0001-01-01T00:00:00Z`, in a spelling that would otherwise read as a
/// date in the year one.
const NEVER: i64 = -62135596800;

/// One row of podman's list, kept apart from rastro's meaning.
#[derive(Debug, Clone, Deserialize)]
pub struct PodmanContainerRow {
    #[serde(rename = "Id")]
    id: String,
    /// A list for docker compatibility; podman holds one name per container, so more than
    /// one means the answer was misread rather than that a container has two.
    #[serde(rename = "Names", default)]
    names: Vec<String>,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "ImageID")]
    image_id: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "ExitCode", default)]
    exit_code: i64,
    #[serde(rename = "Created")]
    created: i64,
    #[serde(rename = "StartedAt", default)]
    started_at: i64,
    #[serde(rename = "ExitedAt", default)]
    exited_at: i64,
    #[serde(rename = "Restarts", default)]
    restarts: i64,
    #[serde(rename = "Pod", default)]
    pod: String,
    #[serde(rename = "IsInfra", default)]
    is_infra: bool,
    #[serde(rename = "AutoRemove", default)]
    auto_remove: bool,
    /// Null on a container with none, which `default` covers either way.
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
    #[serde(rename = "Networks", default)]
    networks: Option<Vec<String>>,
    #[serde(rename = "Ports", default)]
    ports: Option<Vec<PortRow>>,
}

#[derive(Debug, Clone, Deserialize)]
struct PortRow {
    host_ip: String,
    container_port: u16,
    host_port: u16,
    /// How many consecutive ports the binding covers.
    #[serde(default = "one")]
    range: i64,
    protocol: String,
}

fn one() -> i64 {
    1
}

impl PodmanContainerRow {
    /// Translates podman's row into rastro's model, keyed by the name it will sit under.
    pub fn to_container(&self) -> Result<(ContainerName, PodmanContainer), CollectionError> {
        if self.names.len() > 1 {
            return Err(CollectionError::new(format!(
                "podman reported the container {:?} under {} names, and it holds one per \
                 container, so the answer was misread",
                self.id,
                self.names.len()
            )));
        }

        let name = self.names.first().ok_or_else(|| {
            CollectionError::new(format!(
                "podman reported the container {:?} with no name at all",
                self.id
            ))
        })?;

        let mut labels = BTreeMap::new();
        for (label, value) in self.labels.iter().flatten() {
            labels.insert(LabelName::new(label.clone())?, value.clone());
        }

        let mut networks = Vec::new();
        for network in self.networks.iter().flatten() {
            networks.push(NonEmptyText::new(network.clone(), "network name")?);
        }
        // Sorted, because the engine lists them in its own order and promises none.
        networks.sort();

        let container = PodmanContainer {
            id: ContainerId::new(self.id.clone())?,
            image: ImageReference::new(self.image.clone())?,
            image_id: ImageDigest::new(self.image_id.clone())?,
            state: ContainerStatus::new(self.state.clone())?,
            exit_code: self.exit_code,
            created_seconds_since_epoch: self.created,
            started_seconds_since_epoch: happened(self.started_at),
            exited_seconds_since_epoch: happened(self.exited_at),
            restarts: self.restarts,
            pod: NonEmptyText::new(self.pod.clone(), "pod id").ok(),
            is_infra: self.is_infra,
            auto_remove: self.auto_remove,
            labels,
            networks,
            ports: self.ports()?,
        };

        Ok((ContainerName::new(name.clone())?, container))
    }

    /// The published ports, keyed the way the engine names a port.
    fn ports(&self) -> Result<BTreeMap<ExposedPort, Vec<PodmanPortBinding>>, CollectionError> {
        let mut ports: BTreeMap<ExposedPort, Vec<PodmanPortBinding>> = BTreeMap::new();

        for row in self.ports.iter().flatten() {
            let port = ExposedPort::new(
                PortNumber::parse(&row.container_port.to_string())?,
                TransportProtocol::new(row.protocol.clone())?,
            );

            ports.entry(port).or_default().push(PodmanPortBinding {
                host_address: InetHost::new(row.host_ip.clone())?,
                host_port: PortNumber::parse(&row.host_port.to_string())?,
                range: row.range,
            });
        }

        for bindings in ports.values_mut() {
            bindings.sort();
        }

        Ok(ports)
    }
}

/// A stamp podman filled in, or absent for the zero time it prints when nothing has.
fn happened(seconds: i64) -> Option<i64> {
    match seconds == NEVER || seconds == 0 {
        true => None,
        false => Some(seconds),
    }
}
