//! Every server on the box, each asked what discovery says it may be asked.

use std::path::Path;

use rastro_collector::CollectionError;

use super::installed_servers::InstalledServers;
use super::reply::Reply;
use super::resp_connection::RespConnection;
use super::server_discovery::{DiscoveredServer, discover};
use crate::collectors::redis::model::{Installation, Instance};

/// The redis on the box behind `proc`.
///
/// **Never fails as a whole.** Each server's failure is its own `error`, so one wedged or
/// password-protected instance costs its own entry and not its neighbours'.
pub fn read_installation(proc: &Path, installed: &InstalledServers) -> Installation {
    Installation {
        installed: installed.kinds().clone(),
        instances: discover(proc)
            .into_iter()
            .map(|server| (server.key.clone(), read_instance(server)))
            .collect(),
    }
}

fn read_instance(server: DiscoveredServer) -> Instance {
    let error = match &server.reach {
        Ok(target) => asked(RespConnection::dial(target))
            .err()
            .map(|error| error.to_string()),
        Err(refusal) => Some(refusal.clone()),
    };

    Instance {
        server: server.kind,
        listening: server.listeners,
        error,
    }
}

fn asked(connection: Result<RespConnection, CollectionError>) -> Result<(), CollectionError> {
    let mut connection = connection?;

    match connection.ask(&["PING"])? {
        Reply::Simple(pong) if pong == "PONG" => Ok(()),
        Reply::Error(message) if message.starts_with("NOAUTH") => Err(CollectionError::new(
            "the server requires a password, and none was given",
        )),
        Reply::Error(message) => Err(CollectionError::new(format!(
            "the server refused PING: {message}"
        ))),
        other => Err(CollectionError::new(format!(
            "the server answered PING with {other:?} rather than PONG"
        ))),
    }
}
