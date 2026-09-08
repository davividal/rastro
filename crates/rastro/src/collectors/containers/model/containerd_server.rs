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
    /// Where the content store and the snapshots are, and where the running tasks' shims
    /// and sockets are.
    ///
    /// **Reported as values, not only claimed.** The filesystem claim over them can be
    /// folded away when a docker root already seals the tree they sit in, and then the
    /// effective table would be the only place they had ever appeared. They are state in
    /// their own right: a containerd moved to another disk is a change to the box.
    pub root: Option<AbsolutePath>,
    pub state: Option<AbsolutePath>,
    pub namespaces: ContainerdNamespaces,
}

fn directory(path: Option<&AbsolutePath>) -> Observation {
    match path {
        Some(path) => Observation::text(path.as_str()),
        None => Observation::null(),
    }
}

impl From<&ContainerdServer> for Observation {
    fn from(server: &ContainerdServer) -> Self {
        Observation::object([
            ("address", Observation::text(server.address.as_str())),
            ("namespaces", Observation::from(&server.namespaces)),
            ("revision", Observation::text(server.revision.as_str())),
            ("root", directory(server.root.as_ref())),
            ("state", directory(server.state.as_ref())),
            ("version", Observation::from(&server.version)),
        ])
    }
}
