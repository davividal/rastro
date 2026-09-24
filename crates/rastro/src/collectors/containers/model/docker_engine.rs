//! docker on this box: the client that is installed, and the daemon that may not be running.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::model::engine_entry::{optional_server, status_reason};

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

    /// docker installed, with a socket this run may not use.
    pub fn refused(client_version: EngineVersion, reason: NonEmptyText) -> Self {
        Self {
            client_version,
            daemon: DaemonStatus::Refused { reason },
            server: None,
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

impl DockerEngine {
    /// What it holds, or an empty map for a daemon that did not answer.
    ///
    /// Empty rather than absent: the engine's own entry says whether it was read and why,
    /// so a reader who has opened the containers is asking a different question, and an
    /// engine with nothing to show answers it the same way as one holding nothing.
    pub fn containers(&self) -> Observation {
        match &self.server {
            Some(server) => server.containers(),
            None => Observation::object::<&str>([]),
        }
    }
}

impl From<&DockerEngine> for Observation {
    fn from(engine: &DockerEngine) -> Self {
        Observation::object([
            ("client_version", Observation::from(&engine.client_version)),
            ("daemon", Observation::from(&engine.daemon)),
            ("daemon_reason", status_reason(&engine.daemon)),
            ("server", optional_server(engine.server.as_ref())),
        ])
        .incomplete_when(matches!(engine.daemon, DaemonStatus::Refused { .. }))
    }
}
