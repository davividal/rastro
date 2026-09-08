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
//! LXC or incus reads as absent, and so does one running **podman**, which is a limit of
//! rastro rather than a fact about the box. podman is not an oversight: it fails the gate a
//! fingerprint tool has to hold to, because a read initialises its store and every read
//! afterwards writes to a lock and leaves capability probes behind. The measurement, and the
//! two routes that are still open, are in `docs/decisions.md`.
//! The alternative, an unconditional `present`, would put an engine-shaped empty answer into
//! every fingerprint of every box that has never run a container.
//!
//! # What this facet does not model
//!
//! **Owed, and unmeasurable here**: device requests, which is how a GPU reaches a
//! container. There is no GPU on any box this was built against, so there is no fixture for
//! it, and a shape written from the API reference rather than from a run is the mistake this
//! collector's fixtures exist to avoid.
//!
//! **Not owed, because the document already answers it elsewhere.** `Config.ExposedPorts`
//! and `HostConfig.PortBindings` are the request behind the effective port table.
//! `Config.Entrypoint` and `Config.Cmd` are the inputs the recorded command resolves from.
//! `Config.Hostname` is the container's own short id. `State.Pid` and the endpoint id are
//! handles that move on their own, and the link from a process to its container is already
//! readable the other way round, from the control group the `processes` facet records.
//! `GraphDriver` names the layer directories, which the filesystem claim covers as a tree.
//!
//! **Not available rather than not modelled**, per dialect: a containerd image has no size,
//! because the only figure `ctr` prints is a rounded human string, and no labels, because
//! the only form is one comma-joined cell whose values may themselves hold commas. docker's
//! own image entry carries both properly.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    AddressPool, CgroupControl, ContainerCapabilities, ContainerCommand, ContainerDevice,
    ContainerEngine, ContainerEngines, ContainerEnvironment, ContainerHealthcheck, ContainerImage,
    ContainerLabels, ContainerLimits, ContainerLogging, ContainerMount, ContainerMounts,
    ContainerNamespaces, ContainerNetwork, ContainerNetworks, ContainerPorts, ContainerSecurity,
    ContainerState, ContainerdContainer, ContainerdEngine, ContainerdImage, ContainerdNamespace,
    ContainerdNamespaces, ContainerdServer, ContainerdTask, DockerContainer, DockerContainers,
    DockerEngine, DockerImage, DockerImages, DockerNetwork, DockerNetworks, DockerServer,
    DockerVolume, DockerVolumes, ImagePlatform, NameResolution, NetworkAddressing, ObservedHealth,
    PublishedBinding, ResourceLimit, RestartPolicy, UnreadableObject,
};
pub use source::{Containerd, ContainerdLayout, Docker, EngineSource};
pub use value_objects::{
    Capability, ContainerAccount, ContainerId, ContainerName, ContainerStatus, DaemonStatus,
    EngineFlavour, EngineInstant, EngineVersion, ExposedPort, ImageDigest, ImageReference,
    LabelName, MountKind, NamespaceName, NetworkId, NetworkName, StorageDriver, SwarmState,
    TransportProtocol, VariableName, VolumeName,
};

// One import, because `rastro-collector` re-exports what an author needs. A collector written
// outside this repo looks exactly like this.
use rastro_collector::{
    AbsolutePath, CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
};

/// Whether one claimed tree holds another.
///
/// Compared as trees rather than through [`WalkedTree::contains`], which answers about a
/// path inside a tree: the question here is whether a *rule* is redundant, and
/// `/var/lib/dockerx` must not read as being inside `/var/lib/docker`.
fn contains_tree(parent: &WalkedTree, child: &WalkedTree) -> bool {
    match AbsolutePath::new(child.as_str(), "claimed tree") {
        Ok(path) => parent.contains(&path),
        Err(_) => false,
    }
}

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

    /// Every tree an engine on this box keeps to itself, sealed.
    ///
    /// The trees are resolved from each engine's own root rather than named, and what is in
    /// them is either the engine's private bookkeeping or state this facet reports properly.
    /// The one tree deliberately left to the walk is the operator's own data. See
    /// [`Docker::private_trees`].
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        let mut trees: Vec<WalkedTree> = self
            .engines
            .iter()
            .flat_map(EngineSource::private_trees)
            .collect();
        // Shallowest first, so a tree is only ever folded into a parent that has already
        // been kept rather than into a child that happened to be seen first.
        trees.sort_by_key(|tree| tree.as_str().len());

        let mut claimed: Vec<FilesystemClaim> = Vec::new();

        for tree in trees {
            // **One tree is claimed once, whichever dialect resolved it.** Two claims on one
            // path fail the *walk*, not a facet, and two engines on a box can legitimately
            // resolve to the same directory: docker's managed containerd keeps its store
            // inside docker's own root. Saying the same thing twice is not a disagreement,
            // so it is folded rather than reported.
            // **A tree inside a tree already sealed can never apply**, because the walk
            // prunes at the parent and never asks about anything below it. Keeping the
            // deeper rule would put a line in the effective table that no path can ever
            // match: on a docker box the managed containerd keeps its store at
            // `/var/lib/docker/containerd/daemon`, inside docker's own root. Where those
            // directories are is reported as state by the containerd facet instead, so
            // folding the rule loses nothing.
            //
            // Two claims on one path would fail the *walk* rather than one facet, which is
            // the sharper reason this loop exists at all.
            if claimed
                .iter()
                .any(|claim| claim.tree() == &tree || contains_tree(claim.tree(), &tree))
            {
                continue;
            }

            claimed.push(FilesystemClaim::sealed(tree));
        }

        claimed
    }
}
