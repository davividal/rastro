//! The RabbitMQ on this box: its port mapper, its processes and its nodes.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::rabbitmq::model::Node;

/// What this box holds of RabbitMQ, keyed by node.
///
/// **The three parts are reported side by side rather than folded into a verdict**, because
/// they can disagree and the disagreement is the finding. A registered node with no broker
/// process is a stale registration; a broker process with no registered node is a node that
/// has not finished coming up, or one whose register was lost. Folding them into "running"
/// would hide both.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Installation {
    port_mapper_running: bool,

    /// How many processes on the box booted RabbitMQ.
    ///
    /// A count rather than the process ids, which change on every restart of a box nobody
    /// touched and would put noise in every diff.
    broker_processes: usize,

    /// The nodes, keyed by the name the register knows them by.
    ///
    /// **The register's name rather than the node's own**, which is not a preference for the
    /// shorter one: epmd names every node on the box and always answers, while a node's full
    /// name is read from files that an unprivileged run cannot see. Keying on what is always
    /// there keeps one key shape, and the name the node runs under is inside the entry where
    /// it can be absent without leaving a node unkeyed.
    nodes: BTreeMap<String, Node>,
}

impl Installation {
    pub fn new(
        port_mapper_running: bool,
        broker_processes: usize,
        nodes: impl IntoIterator<Item = (String, Node)>,
    ) -> Self {
        Self {
            port_mapper_running,
            broker_processes,
            nodes: nodes.into_iter().collect(),
        }
    }

    pub fn port_mapper_running(&self) -> bool {
        self.port_mapper_running
    }

    pub fn broker_processes(&self) -> usize {
        self.broker_processes
    }

    pub fn nodes(&self) -> &BTreeMap<String, Node> {
        &self.nodes
    }
}

impl From<&Installation> for Observation {
    /// The order here is the author's reading order and not the document's: an observation
    /// object is a sorted map, because a collector's own shape is the half of the contract
    /// the format decides rather than the collector.
    fn from(installation: &Installation) -> Self {
        Observation::object([
            (
                "port_mapper_running",
                Observation::boolean(installation.port_mapper_running()),
            ),
            (
                "broker_processes",
                match i64::try_from(installation.broker_processes()) {
                    Ok(count) => Observation::integer(count),
                    Err(_) => Observation::null(),
                },
            ),
            (
                "nodes",
                Observation::object(
                    installation
                        .nodes()
                        .iter()
                        .map(|(name, node)| (name.as_str(), Observation::from(node))),
                ),
            ),
        ])
    }
}
