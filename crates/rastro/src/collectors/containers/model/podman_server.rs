//! What a podman service says it is.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::containers::model::PodmanStore;
use crate::collectors::containers::value_objects::EngineVersion;

/// The answering service's own account of itself.
///
/// **A service rather than a daemon, and the word matters here.** podman is daemonless by
/// design: nothing is running unless an operator chose to run `podman system service`, and
/// on a box full of running containers there is usually no podman process at all. So this is
/// not "the engine" in the sense docker's daemon is — it is a client-facing API somebody
/// opted into, and everything rastro knows about a podman box beyond its configuration comes
/// through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodmanServer {
    pub version: EngineVersion,
    /// The API version, which is not the same number: the reference box answered with a
    /// service on 6.0.2 while its own client binary was 5.8.6.
    pub api_version: Option<EngineVersion>,
    /// Where rastro asked, which is the value that says *which* service this is on a box
    /// running one per user.
    pub socket: AbsolutePath,
    pub store: PodmanStore,
    /// The OCI runtime that actually starts containers: `crun` on Fedora, `runc` elsewhere.
    pub oci_runtime: Option<NonEmptyText>,
    pub cgroup_version: Option<NonEmptyText>,
    pub cgroup_manager: Option<NonEmptyText>,
    /// `sqlite` or `boltdb`, which says which of two on-disk shapes the state is in.
    pub database_backend: Option<NonEmptyText>,
}

impl From<&PodmanServer> for Observation {
    fn from(server: &PodmanServer) -> Self {
        Observation::object([
            (
                "api_version",
                match &server.api_version {
                    Some(version) => Observation::from(version),
                    None => Observation::null(),
                },
            ),
            ("cgroup_manager", text(server.cgroup_manager.as_ref())),
            ("cgroup_version", text(server.cgroup_version.as_ref())),
            ("database_backend", text(server.database_backend.as_ref())),
            ("oci_runtime", text(server.oci_runtime.as_ref())),
            ("socket", Observation::text(server.socket.as_str())),
            ("store", Observation::from(&server.store)),
            ("version", Observation::from(&server.version)),
        ])
    }
}

fn text(value: Option<&NonEmptyText>) -> Observation {
    match value {
        Some(value) => Observation::text(value.as_str()),
        None => Observation::null(),
    }
}
