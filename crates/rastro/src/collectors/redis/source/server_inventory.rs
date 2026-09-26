//! Every server on the box, each asked what discovery says it may be asked.

use std::path::Path;

use rastro_collector::CollectionError;

use super::config_get::ConfigGet;
use super::info_server::InfoServer;
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
    let mut instance = Instance {
        process_kind: server.kind,
        listening: server.listeners,
        identity: None,
        settings: None,
        errors: Vec::new(),
    };

    let connected = match &server.reach {
        Ok(target) => RespConnection::dial(target),
        Err(refusal) => Err(CollectionError::new(refusal.as_str())),
    };
    let mut connection = match connected {
        Ok(connection) => connection,
        Err(error) => {
            instance.errors.push(error.to_string());
            return instance;
        }
    };

    // **`INFO server` first, and it doubles as the probe.** A server wanting a password answers
    // it `NOAUTH` just as it would `PING`, and a server that will not say what it is has nothing
    // else worth asking.
    match text_reply(&mut connection, &["INFO", "server"]).and_then(|text| InfoServer::parse(&text))
    {
        Ok(identity) => instance.identity = Some(identity),
        Err(error) => {
            instance.errors.push(error.to_string());
            return instance;
        }
    }

    // A refusal from here on is one item's: `rename-command CONFIG ""` leaves `INFO` answering.
    match reply_to(&mut connection, &["CONFIG", "GET", "*"]).and_then(ConfigGet::parse) {
        Ok(settings) => instance.settings = Some(settings),
        Err(error) => instance.errors.push(error.to_string()),
    }

    instance
}

/// A command's answer, with the server's refusals said in words.
fn reply_to(connection: &mut RespConnection, command: &[&str]) -> Result<Reply, CollectionError> {
    let name = command.join(" ");

    match connection.ask(command)? {
        Reply::Error(message) if message.starts_with("NOAUTH") => Err(CollectionError::new(
            "the server requires a password, and none was given",
        )),
        Reply::Error(message) => Err(CollectionError::new(format!(
            "the server refused {name}: {message}"
        ))),
        reply => Ok(reply),
    }
}

/// A command whose answer is text.
fn text_reply(
    connection: &mut RespConnection,
    command: &[&str],
) -> Result<String, CollectionError> {
    match reply_to(connection, command)? {
        Reply::Bulk(text) => Ok(text),
        other => Err(CollectionError::new(format!(
            "the server answered {} with {other:?} rather than text",
            command.join(" ")
        ))),
    }
}
