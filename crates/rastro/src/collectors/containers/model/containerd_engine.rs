//! containerd on this box.

use rastro_collector::Observation;

use crate::collectors::containers::model::ContainerdServer;
use crate::collectors::containers::value_objects::{DaemonStatus, EngineVersion};

/// One containerd, in the same three-part shape docker's entry has: the client that is
/// installed, whether anything answered, and what it said.
///
/// The shape is shared with docker deliberately, even though almost nothing inside it is:
/// the three states a reader has to tell apart — no engine, an engine with nothing
/// answering, and an engine that answered — are the same question whichever engine it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerdEngine {
    pub client_version: EngineVersion,
    pub daemon: DaemonStatus,
    pub server: Option<ContainerdServer>,
}

impl ContainerdEngine {
    pub fn answering(client_version: EngineVersion, server: ContainerdServer) -> Self {
        Self {
            client_version,
            daemon: DaemonStatus::Answering,
            server: Some(server),
        }
    }

    pub fn unreachable(client_version: EngineVersion, reason: &str) -> Self {
        Self {
            client_version,
            daemon: DaemonStatus::unreachable(reason),
            server: None,
        }
    }
}

impl From<&ContainerdEngine> for Observation {
    fn from(engine: &ContainerdEngine) -> Self {
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
