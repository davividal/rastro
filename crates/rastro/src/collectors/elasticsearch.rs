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

pub use model::{Installation, Node, NodeIdentity, NodeVersion};
pub use source::{
    HttpClient, NodeListener, NodeNamespace, NodeSettings, ResidentNode, http_endpoint, read_node,
};
pub use value_objects::{HttpEndpoint, NetworkNamespace, Transport, Unread};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

/// The launcher every package of Elasticsearch installs, deb, rpm and tarball alike.
const INSTALLED_LAUNCHER: &str = "/usr/share/elasticsearch/bin/elasticsearch";

pub struct ElasticsearchCollector {
    name: FacetName,
    identity: CollectorIdentity,
    proc: PathBuf,

    /// Whether the host installed Elasticsearch, which a node in a container does not need.
    installed: bool,
    client: HttpClient,
}

impl ElasticsearchCollector {
    pub fn new() -> Self {
        Self::reading(
            Path::new("/proc"),
            Path::new(INSTALLED_LAUNCHER).exists(),
            HttpClient::new(),
        )
    }

    /// The same collector over sources the caller chose.
    pub fn reading(proc: &Path, installed: bool, client: HttpClient) -> Self {
        Self {
            name: FacetName::new("elasticsearch").expect("`elasticsearch` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("elasticsearch").expect("`elasticsearch` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            proc: proc.to_path_buf(),
            installed,
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

    /// `present` where Elasticsearch is installed **or** a node runs, `absent` only where
    /// neither.
    ///
    /// A node in a container runs on this box and is installed by nothing on it, so presence by
    /// installation alone would hide exactly the node the field host runs. Neither answer is
    /// `Undetermined`: a node that cannot be read surfaces as that node's `error`.
    fn presence(&self) -> Presence {
        match self.installed || !ResidentNode::all_in(&self.proc).is_empty() {
            true => Presence::Present,
            false => Presence::Absent,
        }
    }

    /// Shared, the default, and for a reason the RabbitMQ facet could not claim: a request
    /// leaves an established connection and no listener, and `sockets` records listeners only,
    /// so no other collector can see this one working.
    fn collect(&self) -> Result<Observation, CollectionError> {
        let mut nodes: Vec<Node> = ResidentNode::all_in(&self.proc)
            .iter()
            .map(|resident| read_node(&self.proc, resident, &self.client))
            .collect();
        nodes.sort_by(Node::ordering);

        Ok(Observation::from(&Installation {
            installed: self.installed,
            nodes,
        }))
    }
}
