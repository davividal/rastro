//! Layer 3: what an Elasticsearch node is actually running with.
//!
//! **The node's own HTTP API is the only place its effective state lives**, so this is the one
//! collector that makes a request over the network, and the boundary it works inside is
//! narrow: a `GET`, to a listener held by a process already found in `/proc`, from inside
//! that process's network namespace, and never a request that could write. See
//! `docs/decisions.md`.
//!
//! **Nothing is asked blind.** Which port serves HTTP and whether it wants TLS are settled from
//! the box before the first request, because a request to the wrong port or in the wrong
//! protocol is one the node logs.
pub mod model;
pub mod source;
pub mod value_objects;

use std::path::{Path, PathBuf};

pub use model::{
    ClusterSettings, IlmPolicies, IlmPolicy, IndexEntry, Indices, Installation, NamedDefinitions,
    Node, NodeIdentity, NodeLocal, NodeVersion, Plugins, SnapshotRepositories, SnapshotRepository,
    Surface,
};
pub use source::{
    HeldStore, HttpClient, NodeListener, NodeNamespace, NodeSettings, ResidentNode,
    host_directory_of, http_endpoint, read_node,
};
pub use value_objects::{
    ApiCredential, ApiValue, HttpEndpoint, NetworkNamespace, NodeCredential, Release,
    ReleaseSupport, SupportedRelease, Transport, Unread,
};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    ClaimQualifier, CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
};

/// The launcher the deb and rpm packages install. An archive is extracted wherever its operator
/// chose, so this path says nothing about one; see [`Collector::presence`] for what that costs.
const PACKAGE_LAUNCHER: &str = "/usr/share/elasticsearch/bin/elasticsearch";

pub struct ElasticsearchCollector {
    name: FacetName,
    identity: CollectorIdentity,
    proc: PathBuf,

    /// Whether the package layout's launcher is on the host, which a node in a container does
    /// not need and an archive install does not have.
    package_installed: bool,
    client: HttpClient,
}

impl ElasticsearchCollector {
    pub fn new() -> Self {
        Self::authenticating(None)
    }

    /// The box's collector, sending `credential` to every node it asks.
    pub fn authenticating(credential: Option<ApiCredential>) -> Self {
        Self::reading(
            Path::new("/proc"),
            Path::new(PACKAGE_LAUNCHER).exists(),
            HttpClient::new().authenticating(credential),
        )
    }

    /// The same collector over sources the caller chose.
    pub fn reading(proc: &Path, package_installed: bool, client: HttpClient) -> Self {
        Self {
            name: FacetName::new("elasticsearch").expect("`elasticsearch` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("elasticsearch").expect("`elasticsearch` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            proc: proc.to_path_buf(),
            package_installed,
            client,
        }
    }
}

impl Default for ElasticsearchCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ElasticsearchCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `present` where a node runs **or** the package layout is installed, `absent` where
    /// neither.
    ///
    /// A node in a container runs on this box and is installed by nothing on it, so presence by
    /// installation alone would hide exactly the node the field host runs. Neither answer is
    /// `Undetermined`: a node that cannot be read surfaces as that node's `error`.
    ///
    /// **A stopped archive install reads `absent`**, a limit of rastro rather than a fact about
    /// the box, found by review. An archive is extracted wherever its operator chose, and finding
    /// one that is not running would mean searching the disk for it, which is a guess. A running
    /// one is found from `/proc` wherever it lives.
    fn presence(&self) -> Presence {
        let census = ResidentNode::census_in(&self.proc);
        match (
            self.package_installed || !census.nodes.is_empty(),
            census.some_processes_unseen,
        ) {
            (true, _) => Presence::Present,
            (false, false) => Presence::Absent,
            // Found by the second domain review: a node among processes rastro could not inspect,
            // under `hidepid=1` say, was reported `absent`, a confident claim about a box it
            // could not see.
            (false, true) => Presence::Undetermined {
                reason: "some processes could not be inspected, so a node among them would not \
                         be found"
                    .to_owned(),
            },
        }
    }

    /// Each running node's data directories, sealed.
    ///
    /// **Sealed**, the strongest claim, for the reason the PostgreSQL and RabbitMQ stores are:
    /// measured on 7.17, a node with no request at all moved every index's translog checkpoint
    /// and retention-lease file within ninety seconds. What is in there, the indices and their
    /// schemas, this facet reports properly, from the node.
    ///
    /// Only where the node's data path is the same directory on the host, by device and inode,
    /// which a packaged node's is even in the mount namespace its unit's `PrivateTmp` gives it,
    /// and a node in a container's is not: its path names a directory in its own image.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        ResidentNode::all_in(&self.proc)
            .iter()
            .filter_map(|node| {
                let held = HeldStore::of_in(&self.proc, node.process_id(), node.release())
                    .unwrap_or_default();
                let settings = || NodeSettings::read_in(&self.proc, node).ok();
                // Each held independently: a node can hold its data lock and no log, found by review.
                let data = match held.data.is_empty() {
                    true => settings()?.data_directories(node.home()),
                    false => held.data,
                };
                let logs = match held.logs.is_empty() {
                    true => settings()
                        .map(|settings| settings.log_directories(node.home()))
                        .unwrap_or_default(),
                    false => held.logs,
                };
                let directories: Vec<PathBuf> = data.into_iter().chain(logs).collect();
                // Named by the config directory, the field that leads to the node in `nodes`, so
                // a directory two nodes point at says which two.
                let qualifier = node
                    .config()
                    .and_then(|config| ClaimQualifier::new(config.to_string_lossy()).ok());

                let claims: Vec<FilesystemClaim> = directories
                    .iter()
                    .filter_map(|directory| {
                        host_directory_of(&self.proc, node.process_id(), directory)
                    })
                    .filter_map(|directory| WalkedTree::new(directory.to_string_lossy()).ok())
                    .map(FilesystemClaim::sealed)
                    .map(|claim| match &qualifier {
                        Some(qualifier) => claim.for_entry(qualifier.clone()),
                        None => claim,
                    })
                    .collect();
                Some(claims)
            })
            .flatten()
            .collect()
    }

    /// Shared, the default, and for a reason the RabbitMQ facet could not claim: a request
    /// leaves an established connection and no listener, and `sockets` records listeners only,
    /// so no other collector can see this one working.
    fn collect(&self) -> Result<Observation, CollectionError> {
        let census = ResidentNode::census_in(&self.proc);
        let mut nodes: Vec<Node> = census
            .nodes
            .iter()
            .map(|resident| read_node(&self.proc, resident, &self.client))
            .collect();
        nodes.sort_by(Node::ordering);

        Ok(Observation::from(&Installation {
            package_installed: self.package_installed,
            nodes,
            uninspected_processes: census.some_processes_unseen,
        }))
    }
}
