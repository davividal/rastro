//! docker on this box: the client that is installed, and the daemon that may not be running.

use rastro_collector::Observation;

use crate::collectors::containers::model::DockerServer;
use crate::collectors::containers::value_objects::{DaemonStatus, EngineVersion};

/// One docker installation as rastro means it.
///
/// **The client version and the daemon's are separate fields on purpose.** They are usually
/// equal and the interesting boxes are the ones where they are not: a client upgraded by a
/// package update while the daemon carries on as the old build is a real and awkward state,
/// and one that a single `version` field would hide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerEngine {
    pub client_version: EngineVersion,
    pub daemon: DaemonStatus,
    /// Absent whenever the daemon did not answer, which is structural rather than a
    /// convention: there is nothing to say about a server that was not there to ask.
    pub server: Option<DockerServer>,
}

impl DockerEngine {
    /// docker installed, with a daemon that answered.
    pub fn answering(client_version: EngineVersion, server: DockerServer) -> Self {
        Self {
            client_version,
            daemon: DaemonStatus::Answering,
            server: Some(server),
        }
    }

    /// docker installed, with nothing answering on the socket.
    pub fn unreachable(client_version: EngineVersion, reason: &str) -> Self {
        Self {
            client_version,
            daemon: DaemonStatus::unreachable(reason),
            server: None,
        }
    }
}

impl From<&DockerEngine> for Observation {
    fn from(engine: &DockerEngine) -> Self {
        Observation::object([
            ("client_version", Observation::from(&engine.client_version)),
            ("daemon", Observation::from(&engine.daemon)),
            (
                "daemon_reason",
                match engine.daemon.reason() {
                    Some(reason) => Observation::text(reason),
                    None => Observation::null(),
                },
            ),
            (
                "server",
                match &engine.server {
                    Some(server) => Observation::from(server),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
