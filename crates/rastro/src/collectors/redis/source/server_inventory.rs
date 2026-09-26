//! Every server on the box, each asked what discovery says it may be asked.

use std::path::Path;

use rastro_collector::CollectionError;

use super::config_get::ConfigGet;
use super::info_replication::InfoReplication;
use super::info_server::InfoServer;
use super::installed_servers::InstalledServers;
use super::reply::Reply;
use super::resp_connection::RespConnection;
use super::server_discovery::{DiscoveredServer, discover};
use super::server_password::{Credential, password_for};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::redis::model::{Installation, Instance, ServerIdentity};

/// The first command any server is sent.
const INFO_SERVER: [&str; 2] = ["INFO", "server"];

/// How a server refuses a command until it has a password.
const NOAUTH: &str = "NOAUTH";

/// The redis on the box behind `proc`.
///
/// **Never fails as a whole.** Each server's failure is its own `error`, so one wedged or
/// password-protected instance costs its own entry and not its neighbours'.
pub fn read_installation(
    proc: &Path,
    installed: &InstalledServers,
    systemctl: Option<&CanonicalTool>,
) -> Installation {
    Installation {
        installed: installed.kinds().clone(),
        instances: discover(proc)
            .into_iter()
            .map(|server| {
                let key = server.key.clone();
                (key, read_instance(proc, server, systemctl))
            })
            .collect(),
    }
}

fn read_instance(
    proc: &Path,
    server: DiscoveredServer,
    systemctl: Option<&CanonicalTool>,
) -> Instance {
    let process_id = server.process_id;
    let mut instance = Instance {
        process_kind: server.kind,
        listening: server.listeners,
        identity: None,
        settings: None,
        replication: None,
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

    match identify(&mut connection, proc, process_id, systemctl) {
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

    match text_reply(&mut connection, &["INFO", "replication"])
        .and_then(|text| InfoReplication::parse(&text))
    {
        Ok(replication) => instance.replication = Some(replication),
        Err(error) => instance.errors.push(error.to_string()),
    }

    instance
}

/// What the server says it is, authenticating first where it will say nothing without it.
///
/// **`INFO server` first, and it doubles as the probe.** A server wanting a password answers it
/// `NOAUTH` just as it would `PING`, so nothing is sent merely to find out, and a password is
/// only ever sent to a server that asked for one. Sending it to one that did not is an error the
/// server logs.
fn identify(
    connection: &mut RespConnection,
    proc: &Path,
    process_id: u32,
    systemctl: Option<&CanonicalTool>,
) -> Result<ServerIdentity, CollectionError> {
    let first = connection.ask(&INFO_SERVER)?;
    let answered = match &first {
        Reply::Error(message) if message.starts_with(NOAUTH) => {
            let credential =
                password_for(proc, process_id, systemctl).map_err(CollectionError::new)?;
            authenticate(connection, &credential)?;
            connection.ask(&INFO_SERVER)?
        }
        _ => first,
    };

    InfoServer::parse(&text_of(&INFO_SERVER, answered)?)
}

/// One `AUTH`, never repeated.
///
/// **A refusal is not retried with anything else.** Each is an entry in the server's `ACL LOG`,
/// and the only password rastro has is the one the server was started with; a refusal means the
/// running server and its start disagree, which is itself the finding.
fn authenticate(
    connection: &mut RespConnection,
    credential: &Credential,
) -> Result<(), CollectionError> {
    match connection.ask(&["AUTH", credential.password.as_str()])? {
        Reply::Simple(ok) if ok == "OK" => Ok(()),
        _ => Err(CollectionError::new(format!(
            "the server refused the password {} sets, so the running server and the way it was \
             started disagree",
            credential.origin
        ))),
    }
}

/// A command's answer, with the server's refusals said in words.
fn reply_to(connection: &mut RespConnection, command: &[&str]) -> Result<Reply, CollectionError> {
    let reply = connection.ask(command)?;

    refusal_of(command, reply)
}

/// A reply, or the refusal it carries said in words.
fn refusal_of(command: &[&str], reply: Reply) -> Result<Reply, CollectionError> {
    let name = command.join(" ");

    match reply {
        Reply::Error(message) if message.starts_with(NOAUTH) => Err(CollectionError::new(
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
    let reply = connection.ask(command)?;

    text_of(command, reply)
}

/// A reply that should be text.
fn text_of(command: &[&str], reply: Reply) -> Result<String, CollectionError> {
    match refusal_of(command, reply)? {
        Reply::Bulk(text) => Ok(text),
        other => Err(CollectionError::new(format!(
            "the server answered {} with {other:?} rather than text",
            command.join(" ")
        ))),
    }
}
