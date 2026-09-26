//! Every server on the box, each asked what discovery says it may be asked.

use std::path::Path;

use rastro_collector::CollectionError;

use super::info_server::InfoServer;
use super::installed_servers::InstalledServers;
use super::reply::Reply;
use super::resp_connection::RespConnection;
use super::server_discovery::{DiscoveredServer, discover};
use crate::collectors::redis::model::{Installation, Instance, ServerIdentity};

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
    let asked = match &server.reach {
        Ok(target) => RespConnection::dial(target).and_then(|mut connection| ask(&mut connection)),
        Err(refusal) => Err(CollectionError::new(refusal.as_str())),
    };

    let (identity, error) = match asked {
        Ok(identity) => (Some(identity), None),
        Err(error) => (None, Some(error.to_string())),
    };

    Instance {
        process_kind: server.kind,
        listening: server.listeners,
        identity,
        error,
    }
}

/// Everything one server is asked, in order.
///
/// **`INFO server` first, and it doubles as the probe.** A server wanting a password answers it
/// `NOAUTH` just as it would `PING`, so nothing is sent merely to find out whether the others
/// may be.
fn ask(connection: &mut RespConnection) -> Result<ServerIdentity, CollectionError> {
    let text = text_reply(connection, &["INFO", "server"])?;

    InfoServer::parse(&text)
}

/// A command whose answer is text, with the server's refusals said in words.
fn text_reply(
    connection: &mut RespConnection,
    command: &[&str],
) -> Result<String, CollectionError> {
    let name = command.join(" ");

    match connection.ask(command)? {
        Reply::Bulk(text) => Ok(text),
        Reply::Error(message) if message.starts_with("NOAUTH") => Err(CollectionError::new(
            "the server requires a password, and none was given",
        )),
        Reply::Error(message) => Err(CollectionError::new(format!(
            "the server refused {name}: {message}"
        ))),
        other => Err(CollectionError::new(format!(
            "the server answered {name} with {other:?} rather than text"
        ))),
    }
}
