//! What a containerd says it is.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::containers::model::ContainerdNamespaces;
use crate::collectors::containers::value_objects::EngineVersion;

/// The answering containerd's own account of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerdServer {
    pub version: EngineVersion,
    /// **Recorded, and it matters more here than for docker.** containerd's version moves
    /// slowly and a distribution's rebuild changes only the revision, so the version alone
    /// would call two different builds the same engine.
    pub revision: NonEmptyText,
    /// Where rastro reached it.
    ///
    /// The value that says *which* containerd this is: on a docker box it listens under
    /// docker's own runtime directory rather than at containerd's default, so the address
    /// tells a containerd docker manages apart from one the operator runs.
    pub address: AbsolutePath,
    pub namespaces: ContainerdNamespaces,
}

impl From<&ContainerdServer> for Observation {
    fn from(server: &ContainerdServer) -> Self {
        Observation::object([
            ("address", Observation::text(server.address.as_str())),
            ("namespaces", Observation::from(&server.namespaces)),
            ("revision", Observation::text(server.revision.as_str())),
            ("version", Observation::from(&server.version)),
        ])
    }
}
