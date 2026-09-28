//! The facet's root.

use rastro_collector::Observation;

use crate::collectors::elasticsearch::model::Node;

/// Elasticsearch on this box: whether the host installed it, and every node running.
///
/// The two are independent on purpose. A node in a container is running and installed by no
/// package on the host, and an installed node that is stopped is running nowhere, and both are
/// ordinary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub installed: bool,

    /// In the order [`Node::ordering`] gives, so two runs list two nodes the same way.
    pub nodes: Vec<Node>,
}

impl From<&Installation> for Observation {
    fn from(installation: &Installation) -> Self {
        Observation::object([
            ("installed", Observation::boolean(installation.installed)),
            (
                "nodes",
                Observation::list(installation.nodes.iter().map(Observation::from)),
            ),
        ])
    }
}
