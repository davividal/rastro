//! One node, as the box knows it before anything has been asked of it.

use rastro_collector::Observation;

/// What is known about a node from the register alone.
///
/// Deliberately thin. Everything a node knows about itself, its version, its listeners, its
/// vhosts and its users, arrives from the node itself and is added here as the reads that
/// fetch it land. What this carries is the part that is true without asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The port this node accepts distribution connections on.
    ///
    /// State rather than noise: it is fixed by configuration, `25672` by default, and a
    /// change to it is a change to how the box can be clustered and administered.
    pub distribution_port: u16,
}

impl From<&Node> for Observation {
    fn from(node: &Node) -> Self {
        Observation::object([(
            "distribution_port",
            Observation::integer(i64::from(node.distribution_port)),
        )])
    }
}
