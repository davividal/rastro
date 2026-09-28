//! One node: what the box knows of it, and what it said when asked.

use std::cmp::Ordering;

use rastro_collector::Observation;

use crate::collectors::elasticsearch::model::node_identity::optional;
use crate::collectors::elasticsearch::model::{
    ClusterSettings, IlmPolicies, Indices, NamedDefinitions, NodeIdentity, SnapshotRepositories,
    Surface, surface_observation,
};
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, NetworkNamespace, Unread};

/// A running Elasticsearch server.
///
/// **Everything established is kept, as far as the read got.** A node whose settings were read
/// and whose port was found but which refused the request still says where it listens, beside
/// the reason it was not read, and the `error` is what tells an operator which step stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Volatile: it moves on every restart, which is not a change to the node.
    pub process_id: u32,

    /// `es.path.conf`, as the node's argv names it.
    pub config_directory: Option<String>,

    pub network_namespace: Option<NetworkNamespace>,
    pub http: Option<HttpEndpoint>,
    pub identity: Option<NodeIdentity>,

    /// Each surface is absent where the node was never asked, which its `error` explains.
    pub cluster_settings: Option<Surface<ClusterSettings>>,
    pub index_templates: Option<Surface<NamedDefinitions>>,
    pub component_templates: Option<Surface<NamedDefinitions>>,
    pub indices: Option<Surface<Indices>>,
    pub ilm_policies: Option<Surface<IlmPolicies>>,
    pub ingest_pipelines: Option<Surface<NamedDefinitions>>,
    pub snapshot_repositories: Option<Surface<SnapshotRepositories>>,
    pub error: Option<Unread>,
}

impl Node {
    /// The order nodes are listed in: by the port they serve on, then where they are configured,
    /// then what they are called, and never by process id, which would reorder two nodes on a
    /// restart.
    pub fn ordering(left: &Self, right: &Self) -> Ordering {
        let port = |node: &Self| node.http.as_ref().map(|http| http.port().as_u16());
        let name = |node: &Self| {
            node.identity
                .as_ref()
                .map(|identity| identity.node_name.clone())
        };

        port(left)
            .cmp(&port(right))
            .then_with(|| left.config_directory.cmp(&right.config_directory))
            .then_with(|| name(left).cmp(&name(right)))
    }
}

impl From<&Node> for Observation {
    fn from(node: &Node) -> Self {
        let identity = node.identity.as_ref();

        Observation::object([
            (
                "process_id",
                Observation::integer(i64::from(node.process_id)).volatile(),
            ),
            (
                "config_directory",
                optional(node.config_directory.as_deref()),
            ),
            (
                "network_namespace",
                optional(node.network_namespace.map(|namespace| namespace.as_str())),
            ),
            (
                "http",
                match &node.http {
                    Some(http) => Observation::object([
                        ("host", Observation::from(http.host())),
                        (
                            "port",
                            Observation::integer(i64::from(http.port().as_u16())),
                        ),
                    ]),
                    None => Observation::null(),
                },
            ),
            (
                "node_name",
                optional(identity.map(|identity| identity.node_name.as_str())),
            ),
            (
                "cluster_name",
                optional(identity.map(|identity| identity.cluster_name.as_str())),
            ),
            (
                "cluster_uuid",
                optional(identity.map(|identity| identity.cluster_uuid.as_str())),
            ),
            (
                "version",
                match identity {
                    Some(identity) => Observation::from(&identity.version),
                    None => Observation::null(),
                },
            ),
            (
                "cluster_settings",
                surface_observation(node.cluster_settings.as_ref(), |settings| {
                    Observation::from(settings)
                }),
            ),
            (
                "index_templates",
                surface_observation(node.index_templates.as_ref(), |templates| {
                    Observation::from(templates)
                }),
            ),
            (
                "component_templates",
                surface_observation(node.component_templates.as_ref(), |templates| {
                    Observation::from(templates)
                }),
            ),
            (
                "indices",
                surface_observation(node.indices.as_ref(), |indices| Observation::from(indices)),
            ),
            (
                "ilm_policies",
                surface_observation(node.ilm_policies.as_ref(), |policies| {
                    Observation::from(policies)
                }),
            ),
            (
                "ingest_pipelines",
                surface_observation(node.ingest_pipelines.as_ref(), |pipelines| {
                    Observation::from(pipelines)
                }),
            ),
            (
                "snapshot_repositories",
                surface_observation(node.snapshot_repositories.as_ref(), |repositories| {
                    Observation::from(repositories)
                }),
            ),
            ("error", optional(node.error.as_ref().map(Unread::reason))),
        ])
        .incomplete_when(node.error.is_some())
    }
}
