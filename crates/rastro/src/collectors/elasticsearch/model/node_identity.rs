//! What a node says it is.

use rastro_collector::Observation;

/// Which node this is, of which cluster, running what.
///
/// `cluster_uuid` is kept: it is fixed when a cluster first forms and changes only when its
/// data is wiped and it forms again, which is exactly a change a before-and-after pair should
/// show. `node_name` defaults to the host name, so in a container it is the container's id and
/// moves when the container is recreated, which is also a real change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIdentity {
    pub node_name: String,
    pub cluster_name: String,
    pub cluster_uuid: String,
    pub version: NodeVersion,
}

/// The build a node runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeVersion {
    pub number: String,
    pub build_flavor: Option<String>,

    /// `deb`, `rpm`, `tar` or `docker`: how the node was installed, as its build says.
    pub build_type: Option<String>,
    pub build_hash: Option<String>,
}

impl From<&NodeVersion> for Observation {
    fn from(version: &NodeVersion) -> Self {
        Observation::object([
            ("number", Observation::text(&version.number)),
            ("build_flavor", optional(version.build_flavor.as_deref())),
            ("build_type", optional(version.build_type.as_deref())),
            ("build_hash", optional(version.build_hash.as_deref())),
        ])
    }
}

pub(super) fn optional(value: Option<&str>) -> Observation {
    match value {
        Some(value) => Observation::text(value),
        None => Observation::null(),
    }
}
