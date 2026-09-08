//! Layer 3: the container engines on the box, and what each of them is running.
//!
//! Three layers, and the dependency arrows only point one way: [`source`] knows [`model`],
//! `model` knows [`value_objects`], and neither of the last two knows a host interface
//! exists.
//!
//! # One facet, several engines
//!
//! **Keyed by engine flavour rather than one facet per engine**, the way `packages` covers
//! dpkg and apk together. Two of them legitimately sit side by side: docker runs containerd
//! underneath itself, so a docker box has both, and both are reported. They describe the same
//! containers at different levels, and keeping them apart is what lets them disagree, which
//! is the same reason the `exporters` facet records the endpoint an agent is configured with
//! separately from what `sockets` observed bound.
//!
//! # An engine that is installed and not running is state
//!
//! Three different facts, and the facet keeps all three apart:
//!
//! - no engine on the box at all: the facet is `absent`;
//! - an engine installed with nothing answering: `ok`, with the engine's `daemon`
//!   `unreachable` and no server node, because there was no server to describe;
//! - rastro unable to look: an `error`, loudly.
//!
//! **Cost, accepted knowingly:** `absent` means "no engine rastro knows of". A box running
//! LXC or incus reads as absent, which is a limit of rastro rather than a fact about the box.
//! The alternative, an unconditional `present`, would put an engine-shaped empty answer into
//! every fingerprint of every box that has never run a container.
//!
//! # What this facet does not model
//!
//! **Owed, and each would be a change an operator cares about**: devices passed in
//! (`--device`), per-container ulimits and sysctls, the DNS configuration (`--dns`,
//! `--dns-search`, `--add-host`), the shared-memory size, and device requests, which is how
//! a GPU reaches a container. Also the box-level objects: images, volumes and networks in
//! their own right rather than as seen from a container.
//!
//! **Not owed, because the document already answers it elsewhere.** `Config.ExposedPorts`
//! and `HostConfig.PortBindings` are the request behind the effective port table.
//! `Config.Entrypoint` and `Config.Cmd` are the inputs the recorded command resolves from.
//! `Config.Hostname` is the container's own short id. `State.Pid` and the endpoint id are
//! handles that move on their own, and the link from a process to its container is already
//! readable the other way round, from the control group the `processes` facet records.
//! `GraphDriver` names the layer directories, which the filesystem claim covers as a tree.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    CgroupControl, ContainerCapabilities, ContainerCommand, ContainerEngine, ContainerEngines,
    ContainerEnvironment, ContainerHealthcheck, ContainerImage, ContainerLabels, ContainerLimits,
    ContainerLogging, ContainerMount, ContainerMounts, ContainerNamespaces, ContainerNetwork,
    ContainerNetworks, ContainerPorts, ContainerSecurity, ContainerState, DockerContainer,
    DockerContainers, DockerEngine, DockerImage, DockerImages, DockerServer, DockerVolume,
    DockerVolumes, ImagePlatform, ObservedHealth, PublishedBinding, RestartPolicy,
    UnreadableObject,
};
pub use source::{Docker, EngineSource};
pub use value_objects::{
    Capability, ContainerAccount, ContainerId, ContainerName, ContainerStatus, DaemonStatus,
    EngineFlavour, EngineInstant, EngineVersion, ExposedPort, ImageDigest, ImageReference,
    LabelName, MountKind, NetworkId, NetworkName, StorageDriver, SwarmState, TransportProtocol,
    VariableName, VolumeName,
};

// One import, because `rastro-collector` re-exports what an author needs. A collector written
// outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct ContainersCollector {
    name: FacetName,
    identity: CollectorIdentity,
    engines: Vec<EngineSource>,
}

impl ContainersCollector {
    /// Detects every engine on the host once, at construction.
    ///
    /// Detecting here rather than inside `presence` is what stops the two disagreeing: what
    /// was found is the very thing `collect` will read.
    pub fn new() -> Self {
        Self::reading(EngineSource::detect_all())
    }

    /// The same collector over sources the caller chose.
    pub fn reading(engines: Vec<EngineSource>) -> Self {
        Self {
            name: FacetName::new("containers").expect("`containers` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("containers").expect("`containers` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            engines,
        }
    }
}

impl Default for ContainersCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ContainersCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `absent` on a box with no engine rastro can read, `present` with one, whether or not
    /// its daemon is answering.
    ///
    /// Deliberately not `Undetermined`: locating a client is a question rastro can answer, so
    /// there is nothing it could not tell. The reasons it genuinely cannot look come back from
    /// [`Collector::collect`] as an `error` instead, by which point the engine is known to be
    /// there.
    fn presence(&self) -> Presence {
        match self.engines.is_empty() {
            true => Presence::Absent,
            false => Presence::Present,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let found = self
            .engines
            .iter()
            .map(EngineSource::read)
            .collect::<Result<Vec<ContainerEngine>, CollectionError>>()?;

        Ok(Observation::from(&ContainerEngines::new(found)?))
    }
}
