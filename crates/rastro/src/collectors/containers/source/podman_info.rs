//! `podman --remote info --format json`: the service's own account of itself.

use serde::Deserialize;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use crate::collectors::containers::model::{PodmanContainers, PodmanServer, PodmanStore};
use crate::collectors::containers::value_objects::{EngineVersion, StorageDriver};

/// podman's document, kept apart from rastro's meaning.
///
/// Only the fields this facet reads are declared. What is left out on purpose: the OCI
/// runtime's multi-line `version` blob and the package names around it, which describe the
/// distribution's packaging rather than the box's state; the registries; and the plugin
/// lists.
#[derive(Debug, Clone, Deserialize)]
pub struct PodmanInfoDocument {
    host: HostHalf,
    store: StoreHalf,
    version: VersionHalf,
}

#[derive(Debug, Clone, Deserialize)]
struct HostHalf {
    #[serde(rename = "ociRuntime", default)]
    oci_runtime: Option<NamedTool>,
    #[serde(rename = "cgroupVersion", default)]
    cgroup_version: String,
    #[serde(rename = "cgroupManager", default)]
    cgroup_manager: String,
    #[serde(rename = "databaseBackend", default)]
    database_backend: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NamedTool {
    #[serde(rename = "name", default)]
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StoreHalf {
    #[serde(rename = "graphRoot", default)]
    graph_root: String,
    #[serde(rename = "runRoot", default)]
    run_root: String,
    #[serde(rename = "volumePath", default)]
    volume_path: String,
    #[serde(rename = "graphDriverName", default)]
    driver: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionHalf {
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "APIVersion", default)]
    api_version: String,
}

impl PodmanInfoDocument {
    /// Translates the service's document into rastro's model, given where it was asked.
    pub fn to_server(
        &self,
        socket: AbsolutePath,
        containers: PodmanContainers,
    ) -> Result<PodmanServer, CollectionError> {
        Ok(PodmanServer {
            version: EngineVersion::new(self.version.version.clone())?,
            api_version: EngineVersion::new(self.version.api_version.clone()).ok(),
            socket,
            store: PodmanStore {
                graph_root: AbsolutePath::new(self.store.graph_root.clone(), "graph root").ok(),
                run_root: AbsolutePath::new(self.store.run_root.clone(), "run root").ok(),
                volume_path: AbsolutePath::new(self.store.volume_path.clone(), "volume path").ok(),
                driver: StorageDriver::new(self.store.driver.clone()).ok(),
            },
            oci_runtime: self
                .host
                .oci_runtime
                .as_ref()
                .and_then(|runtime| NonEmptyText::new(runtime.name.clone(), "oci runtime").ok()),
            cgroup_version: NonEmptyText::new(self.host.cgroup_version.clone(), "cgroup version")
                .ok(),
            cgroup_manager: NonEmptyText::new(self.host.cgroup_manager.clone(), "cgroup manager")
                .ok(),
            database_backend: NonEmptyText::new(
                self.host.database_backend.clone(),
                "database backend",
            )
            .ok(),
            containers,
        })
    }
}
