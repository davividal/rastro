//! One node: what the box knows of it, and what it said when asked.

use std::cmp::Ordering;

use rastro_collector::{Content, Observation, Scalar, Volatility};

use crate::collectors::elasticsearch::model::node_identity::optional;
use crate::collectors::elasticsearch::model::{
    ClusterSettings, IlmPolicies, Indices, NamedDefinitions, NodeIdentity, NodeLocal, Plugins,
    SnapshotRepositories, Surface, surface_observation,
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
    pub plugins: Option<Surface<Plugins>>,
    pub node_local: Option<Surface<NodeLocal>>,
    pub error: Option<Unread>,
}

impl Node {
    /// The order nodes are listed in, by what a node is rather than by its process id, which a
    /// restart of two nodes could swap.
    ///
    /// Port, then configuration directory, then name: what tells most nodes apart. Then the
    /// cluster and the build, found by review: containers can each serve 9200 in their own
    /// namespace, share the image's config directory and carry one name across two clusters.
    /// Then the namespace and the address dialled, and the error last, since an unread node has
    /// none of the rest. Two nodes that still tie are one cluster's two same-named nodes, whose
    /// surfaces are cluster-wide and identical, so their order changes nothing a diff can see.
    pub fn ordering(left: &Self, right: &Self) -> Ordering {
        let port = |node: &Self| node.http.as_ref().map(|http| http.port().as_u16());
        let host = |node: &Self| node.http.as_ref().map(|http| http.host().clone());
        let identity = |node: &Self| {
            node.identity.as_ref().map(|identity| {
                [
                    identity.node_name.clone(),
                    identity.cluster_uuid.clone(),
                    identity.cluster_name.clone(),
                    identity.version.number.clone(),
                ]
            })
        };
        let namespace = |node: &Self| node.network_namespace.map(|namespace| namespace.as_str());
        let reason = |node: &Self| node.error.as_ref().map(|unread| unread.reason().to_owned());

        port(left)
            .cmp(&port(right))
            .then_with(|| left.config_directory.cmp(&right.config_directory))
            .then_with(|| identity(left).cmp(&identity(right)))
            .then_with(|| namespace(left).cmp(&namespace(right)))
            .then_with(|| host(left).cmp(&host(right)))
            .then_with(|| reason(left).cmp(&reason(right)))
            .then_with(|| stable_rendering(left).cmp(&stable_rendering(right)))
    }
}

/// The address a node was dialled on, volatile where an engine assigned it.
///
/// Found by the second domain review, measured: `network.host=_site_` in a container dialled the
/// container's own address, which moves whenever the container is recreated while nothing about
/// the node changed. Loopback is the same wherever the node runs, and a host's address is its
/// own configuration, so those stay.
fn dialled_host(http: &HttpEndpoint, namespace: Option<NetworkNamespace>) -> Observation {
    let host = Observation::from(http.host());
    let assigned = namespace == Some(NetworkNamespace::Separate)
        && !matches!(
            http.host().as_str(),
            "127.0.0.1" | "::1" | "::ffff:127.0.0.1" | "0.0.0.0" | "::"
        );

    match assigned {
        true => host.volatile(),
        false => host,
    }
}

/// A node's rendering as the diffable view shows it, encoded so that two nodes compare equal
/// only where they render the same.
///
/// The comparator's last key, found by review the third time it was: every key added before
/// it named one more thing two nodes could differ in, and `plugins`, read per node, was the
/// next. Ending on the whole rendering closes that for good. Two nodes that still tie render
/// identically, so which one is listed first changes nothing a diff can see. Volatile values are
/// left out, the process id among them, because they are what the diffable view leaves out.
fn stable_rendering(node: &Node) -> Vec<u8> {
    let mut encoded = Vec::new();
    encode_stable(&Observation::from(node), &mut encoded);
    encoded
}

/// Every value tagged with its kind and every text and collection with its length, so that no
/// two different renderings encode alike.
fn encode_stable(observation: &Observation, encoded: &mut Vec<u8>) {
    let length = |encoded: &mut Vec<u8>, length: usize| {
        encoded.extend_from_slice(&(length as u64).to_be_bytes());
    };

    if observation.volatility() == Volatility::Volatile {
        encoded.push(0);
        return;
    }

    match observation.content() {
        Content::Scalar(Scalar::Null) => encoded.push(1),
        Content::Scalar(Scalar::Boolean(flag)) => encoded.extend_from_slice(&[2, u8::from(*flag)]),
        Content::Scalar(Scalar::Integer(number)) => {
            encoded.push(3);
            encoded.extend_from_slice(&number.to_be_bytes());
        }
        Content::Scalar(Scalar::Text(text)) => {
            encoded.push(4);
            length(encoded, text.len());
            encoded.extend_from_slice(text.as_bytes());
        }
        Content::List(items) => {
            encoded.push(5);
            length(encoded, items.len());
            for item in items {
                encode_stable(item, encoded);
            }
        }
        Content::Object(entries) => {
            encoded.push(6);
            length(encoded, entries.len());
            for (key, value) in entries {
                length(encoded, key.len());
                encoded.extend_from_slice(key.as_bytes());
                encode_stable(value, encoded);
            }
        }
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
                        ("host", dialled_host(http, node.network_namespace)),
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
            (
                "plugins",
                surface_observation(node.plugins.as_ref(), |plugins| Observation::from(plugins)),
            ),
            (
                "node_local",
                surface_observation(node.node_local.as_ref(), |local| Observation::from(local)),
            ),
            ("error", optional(node.error.as_ref().map(Unread::reason))),
        ])
        .incomplete_when(node.error.is_some())
    }
}
