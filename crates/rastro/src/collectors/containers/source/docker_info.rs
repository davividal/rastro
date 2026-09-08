//! `docker info --format '{{json .}}'`: what the answering daemon runs with.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use crate::collectors::containers::model::{
    CgroupControl, DockerContainers, DockerImages, DockerNetworks, DockerServer, DockerVolumes,
};
use crate::collectors::containers::value_objects::{EngineVersion, StorageDriver, SwarmState};

/// docker's own field names, which are neither consistently cased nor stable enough to
/// derive from a convention: `DockerRootDir` beside `LiveRestoreEnabled` beside `Driver`.
/// Every one is spelled out.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerInfoDocument {
    #[serde(rename = "Driver")]
    driver: String,
    /// Empty on a daemon that will not say, which is why the model's field is optional.
    #[serde(rename = "DockerRootDir", default)]
    root_directory: String,
    #[serde(rename = "CgroupDriver", default)]
    cgroup_driver: String,
    #[serde(rename = "CgroupVersion", default)]
    cgroup_version: String,
    #[serde(rename = "LoggingDriver", default)]
    logging_driver: String,
    #[serde(rename = "DefaultRuntime", default)]
    default_runtime: String,
    #[serde(rename = "LiveRestoreEnabled", default)]
    live_restore: bool,
    /// Null rather than empty on a daemon with none, which `default` covers either way.
    #[serde(rename = "SecurityOptions", default)]
    security_options: Vec<String>,
    #[serde(rename = "Swarm", default)]
    swarm: Option<SwarmHalf>,
}

#[derive(Debug, Clone, Deserialize)]
struct SwarmHalf {
    #[serde(rename = "LocalNodeState")]
    local_node_state: String,
}

/// The membership reported for a daemon built without swarm support at all.
const NO_SWARM: &str = "unsupported";

impl DockerInfoDocument {
    /// Translates the daemon's report, given the versions the probe already established.
    ///
    /// The versions arrive from the caller rather than being read again here: `docker info`
    /// carries a `ServerVersion` of its own, and reading the same fact from two documents is
    /// how the two come to disagree.
    pub fn to_server(
        &self,
        version: EngineVersion,
        components: Vec<(String, EngineVersion)>,
        containers: DockerContainers,
        images: DockerImages,
        volumes: DockerVolumes,
        networks: DockerNetworks,
    ) -> Result<DockerServer, CollectionError> {
        let mut security_options: Vec<NonEmptyText> = self
            .security_options
            .iter()
            .filter_map(|option| NonEmptyText::new(option.clone(), "security option").ok())
            .collect();
        security_options.sort();

        let named_components: BTreeMap<NonEmptyText, EngineVersion> = components
            .into_iter()
            .filter_map(|(name, version)| {
                NonEmptyText::new(name, "engine component")
                    .ok()
                    .map(|name| (name, version))
            })
            .collect();

        Ok(DockerServer {
            version,
            root_directory: AbsolutePath::new(self.root_directory.clone(), "docker root").ok(),
            storage_driver: StorageDriver::new(self.driver.clone())?,
            cgroup: CgroupControl {
                driver: NonEmptyText::new(self.cgroup_driver.clone(), "cgroup driver")?,
                version: NonEmptyText::new(self.cgroup_version.clone(), "cgroup version")?,
            },
            logging_driver: NonEmptyText::new(self.logging_driver.clone(), "logging driver")?,
            default_runtime: NonEmptyText::new(self.default_runtime.clone(), "default runtime")?,
            live_restore: self.live_restore,
            swarm: SwarmState::new(self.swarm_membership())?,
            security_options,
            components: named_components,
            containers,
            images,
            volumes,
            networks,
        })
    }

    /// Where the daemon keeps its store, for the claim that is made before the facet runs.
    pub fn root_directory(&self) -> Option<AbsolutePath> {
        AbsolutePath::new(self.root_directory.clone(), "docker root").ok()
    }

    /// The swarm word, with a daemon that reports no swarm section named as such.
    ///
    /// A daemon built without swarm support omits the section, and recording that as absent
    /// would read as "rastro did not look". It is a property of the build, so it is spelled.
    fn swarm_membership(&self) -> String {
        match &self.swarm {
            Some(swarm) if !swarm.local_node_state.is_empty() => swarm.local_node_state.clone(),
            _ => NO_SWARM.to_owned(),
        }
    }
}
