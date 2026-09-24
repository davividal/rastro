//! One node: what the box knows of it, and what it said when asked.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::{Definitions, NodeStatus};
use crate::collectors::rabbitmq::value_objects::{BrokerEvidence, NodeName};

/// A node of the Erlang distribution on this box.
///
/// **A registered node is not necessarily a RabbitMQ node**, which is why `runs_rabbitmq` is
/// here and is not merely the answer to "did the read succeed". The register names every
/// Erlang node, so the same list can hold an ejabberd or a CouchDB, and addressing one of
/// those with a RabbitMQ CLI tool would make it log an authentication failure: a write to a
/// box rastro was asked to read. The flag is what rastro established before asking, from the
/// process holding this node's distribution port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The name the node runs under, read from the files it holds open.
    ///
    /// **Absent means rastro could not read it**, not that the node has none: a broker whose
    /// descriptors are unreadable, or one holding none of Ra's directories open. A node that
    /// cannot be named is also one that cannot be addressed, because `rabbitmqctl -n` takes
    /// this and nothing else, and composing it from the box's hostname is what this facet
    /// stopped doing: a node under long names calls itself `rabbit@broker.example.test` and
    /// the composition would have said `rabbit@broker`.
    pub node_name: Option<NodeName>,

    /// The port this node accepts distribution connections on.
    ///
    /// State rather than noise: it is fixed by configuration, `25672` by default, and a
    /// change to it is a change to how the box can be clustered and administered.
    pub distribution_port: u16,

    /// What the box's own evidence says about whether this node is a broker.
    ///
    /// Renders as two keys: `runs_rabbitmq`, which is null where the box could not say, and
    /// the evidence itself in words. One field rather than two, so the answer and the reason
    /// for it cannot drift apart.
    pub evidence: BrokerEvidence,

    /// What the node said about itself, where it was asked and answered.
    ///
    /// Absent where the node was not addressed at all, which the evidence beside it explains,
    /// and where this box has no client to ask with. A broker that was asked and refused
    /// fails the facet instead, because rastro could see it and could not read it.
    pub status: Option<NodeStatus>,

    /// Which feature flags the node has enabled, where it was asked.
    ///
    /// Not volatile and not configuration: enabling one is a deliberate, **irreversible** act
    /// that decides which versions this node can be upgraded to and which nodes it can be
    /// clustered with. A flag appearing as enabled between two fingerprints is one of the
    /// larger changes this facet can report.
    pub feature_flags: Option<BTreeMap<String, String>>,

    /// The durable half: vhosts, users, permissions, policies, parameters and topology.
    pub definitions: Option<Definitions>,
}

impl From<&Node> for Observation {
    fn from(node: &Node) -> Self {
        Observation::object([
            (
                "node_name",
                match &node.node_name {
                    Some(name) => Observation::text(name.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "distribution_port",
                Observation::integer(i64::from(node.distribution_port)),
            ),
            (
                "runs_rabbitmq",
                match node.evidence.runs_rabbitmq() {
                    Some(runs) => Observation::boolean(runs),
                    None => Observation::null(),
                },
            ),
            ("broker_evidence", Observation::text(node.evidence.as_str())),
            (
                "status",
                match &node.status {
                    Some(status) => Observation::from(status),
                    None => Observation::null(),
                },
            ),
            (
                "feature_flags",
                match &node.feature_flags {
                    Some(flags) => {
                        Observation::object(flags.iter().map(|(name, state)| {
                            (name.as_str(), Observation::text(state.as_str()))
                        }))
                    }
                    None => Observation::null(),
                },
            ),
            (
                "definitions",
                match &node.definitions {
                    Some(definitions) => Observation::from(definitions),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
