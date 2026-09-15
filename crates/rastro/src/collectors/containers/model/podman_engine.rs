//! podman on this box.

use rastro_collector::Observation;

use crate::collectors::containers::model::engine_entry::{optional_server, status_reason};

use crate::collectors::containers::model::PodmanServer;
use crate::collectors::containers::value_objects::{DaemonStatus, EngineVersion};

/// One podman installation, in the same three-part shape the other dialects have.
///
/// **The middle state is the common one here, unlike anywhere else in this facet.** docker
/// with no daemon is a box somebody stopped; podman with no service is simply podman, since
/// the service is opt-in and containers run without it. So "installed and not readable" is
/// not an error and not an absence: it is the ordinary condition of a podman host, and the
/// reason is recorded so a reader is never left wondering whether rastro looked.
///
/// What rastro does **not** do in that state is ask podman anyway. A local read initialises
/// the store, and connecting to a socket-activated `podman.socket` would start the service
/// that then does the same thing; `docs/decisions.md` carries both measurements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodmanEngine {
    /// Read with `podman --version`, which is measured not to touch the store. The `version`
    /// subcommand is a different thing and does: it reports a server version, and in local
    /// mode producing one means becoming one.
    pub client_version: EngineVersion,
    pub service: DaemonStatus,
    pub server: Option<PodmanServer>,
}

impl PodmanEngine {
    pub fn answering(client_version: EngineVersion, server: PodmanServer) -> Self {
        Self {
            client_version,
            service: DaemonStatus::Answering,
            server: Some(server),
        }
    }

    pub fn unread(client_version: EngineVersion, reason: &str) -> Self {
        Self {
            client_version,
            service: DaemonStatus::unreachable(reason),
            server: None,
        }
    }
}

impl PodmanEngine {
    /// What it holds, or an empty map where no service answered.
    pub fn containers(&self) -> Observation {
        match &self.server {
            Some(server) => server.containers(),
            None => Observation::object::<&str>([]),
        }
    }
}

impl From<&PodmanEngine> for Observation {
    fn from(engine: &PodmanEngine) -> Self {
        Observation::object([
            ("client_version", Observation::from(&engine.client_version)),
            ("server", optional_server(engine.server.as_ref())),
            ("service", Observation::from(&engine.service)),
            ("service_reason", status_reason(&engine.service)),
        ])
    }
}
