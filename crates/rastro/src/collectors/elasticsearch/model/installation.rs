//! The facet's root.

use rastro_collector::Observation;

use crate::collectors::elasticsearch::model::Node;

/// Elasticsearch on this box: whether the package layout is installed, and every node running.
///
/// The two are independent on purpose. A node in a container is running and installed by no
/// package on the host, and an installed node that is stopped is running nowhere, and both are
/// ordinary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    /// Whether `/usr/share/elasticsearch/bin/elasticsearch`, the deb and rpm launcher, exists.
    ///
    /// Named for what is observed, found by review: an archive install is extracted elsewhere
    /// and reads `false` here while its node, if running, is in `nodes`.
    pub package_installed: bool,

    /// In the order [`Node::ordering`] gives, so two runs list two nodes the same way.
    pub nodes: Vec<Node>,

    /// Whether some process could not be inspected, so a node among them would not be listed.
    ///
    /// Rendered as words with no count, and marked incomplete: how many processes a run was
    /// refused moves between two runs of an unchanged box, which the document must not.
    pub uninspected_processes: bool,
}

impl From<&Installation> for Observation {
    fn from(installation: &Installation) -> Self {
        Observation::object([
            (
                "package_installed",
                Observation::boolean(installation.package_installed),
            ),
            (
                "nodes",
                Observation::list(installation.nodes.iter().map(Observation::from)),
            ),
            (
                "uninspected_processes",
                match installation.uninspected_processes {
                    true => Observation::text(
                        "some processes could not be inspected, so a node among them would not \
                         be listed",
                    )
                    .incomplete(),
                    false => Observation::null(),
                },
            ),
        ])
    }
}
