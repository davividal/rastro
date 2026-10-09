//! Every server on the box, each asked what discovery says it may be asked.

use std::path::Path;

use rastro_collector::CollectionError;

use super::acl_list::AclList;
use super::config_get::ConfigGet;
use super::info_replication::InfoReplication;
use super::info_server::InfoServer;
use super::installed_servers::InstalledServers;
use super::module_list::ModuleList;
use super::reply::{Reply, shortened};
use super::resident_servers::resident_census;
use super::resp_connection::RespConnection;
use super::server_discovery::{DialTarget, DiscoveredServer, discover};
use super::server_password::{Credential, password_for};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::redis::model::{Installation, Instance, ServerIdentity};
use crate::collectors::redis::value_objects::Listener;

/// The first command any server is sent.
const INFO_SERVER: [&str; 2] = ["INFO", "server"];

/// The mode `INFO server` names for a sentinel.
const SENTINEL: &str = "sentinel";

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
        uninspected_processes: resident_census(proc).some_processes_unseen,
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
        acl: None,
        modules: None,
        not_read: Vec::new(),
        errors: Vec::new(),
    };

    let target = match &server.reach {
        Ok(target) => target,
        Err(unreached) => {
            instance.record(match unreached.withheld {
                true => Shortfall::Withheld(unreached.reason.clone()),
                false => Shortfall::Failed(unreached.reason.clone()),
            });
            return instance;
        }
    };

    // A socket keeps the namespace it was made in, so only the connection joins the server's.
    let dialled = match server
        .namespace
        .run(|| RespConnection::dial(target, process_id))
    {
        Ok(dialled) => dialled,
        Err(refusal) => {
            instance.record(match refusal.refused {
                true => Shortfall::Withheld(refusal.reason),
                false => Shortfall::Failed(refusal.reason),
            });
            return instance;
        }
    };
    // Dialling and the first answer are where a TLS port hangs up on a plain client.
    let answered = dialled.and_then(|mut connection| {
        let first = connection.ask(&INFO_SERVER)?;
        Ok((connection, first))
    });
    let (mut connection, first) = match answered {
        Ok(answered) => answered,
        Err(error) => {
            instance.errors.push(format!(
                "{error}{}",
                untried_note(target, &instance.listening)
            ));
            return instance;
        }
    };

    match identify(first, &mut connection, proc, process_id, systemctl) {
        Ok(identity) => instance.identity = Some(identity),
        Err(shortfall) => {
            instance.record(shortfall);
            return instance;
        }
    }

    // A sentinel answers `INFO` and rejects the rest, measured, and each rejection is one more
    // error the server counts; this facet reads one box's servers, not a sentinel's topology.
    let is_sentinel = instance
        .identity
        .as_ref()
        .and_then(|identity| identity.mode.as_deref())
        == Some(SENTINEL);
    if is_sentinel {
        instance.record(Shortfall::Withheld(
            "it is a sentinel, which this facet names and does not read beyond its identity"
                .to_owned(),
        ));
        return instance;
    }

    // A refusal from here on is one item's: `rename-command CONFIG ""` leaves `INFO` answering.
    match reply_to(&mut connection, &["CONFIG", "GET", "*"])
        .and_then(|reply| Ok(ConfigGet::parse(reply)?))
    {
        Ok(settings) => instance.settings = Some(settings),
        Err(shortfall) => instance.record(shortfall),
    }

    match text_reply(&mut connection, &["INFO", "replication"])
        .and_then(|text| Ok(InfoReplication::parse(&text)?))
    {
        Ok(replication) => instance.replication = Some(replication),
        Err(shortfall) => instance.record(shortfall),
    }

    let has_accounts = instance
        .identity
        .as_ref()
        .is_some_and(ServerIdentity::has_accounts);
    if has_accounts {
        match reply_to(&mut connection, &["ACL", "LIST"])
            .and_then(|reply| Ok(AclList::parse(reply)?))
        {
            Ok(acl) => instance.acl = Some(acl),
            Err(shortfall) => instance.record(shortfall),
        }
    }

    match reply_to(&mut connection, &["MODULE", "LIST"])
        .and_then(|reply| Ok(ModuleList::parse(reply)?))
    {
        Ok(modules) => instance.modules = Some(modules),
        Err(shortfall) => instance.record(shortfall),
    }

    instance
}

/// Why one read came to nothing: the box withheld it, or it failed.
///
/// **Two, because "Not read is not an error"**: no privilege, no credential or a refused one, and
/// a command the server refuses are the box's own state, and nothing on it is wrong.
enum Shortfall {
    Withheld(String),
    Failed(String),
}

impl From<CollectionError> for Shortfall {
    fn from(error: CollectionError) -> Self {
        Shortfall::Failed(error.to_string())
    }
}

impl Instance {
    fn record(&mut self, shortfall: Shortfall) {
        match shortfall {
            Shortfall::Withheld(reason) => self.not_read.push(reason),
            Shortfall::Failed(reason) => self.errors.push(reason),
        }
    }
}

/// What the server says it is, authenticating first where it will say nothing without it.
///
/// **`INFO server` first, and it doubles as the probe.** A server wanting a password answers it
/// `NOAUTH` just as it would `PING`, so nothing is sent merely to find out, and a password is
/// only ever sent to a server that asked for one. Sending it to one that did not is an error the
/// server logs.
fn identify(
    first: Reply,
    connection: &mut RespConnection,
    proc: &Path,
    process_id: u32,
    systemctl: Option<&CanonicalTool>,
) -> Result<ServerIdentity, Shortfall> {
    let answered = match &first {
        Reply::Error(message) if message.starts_with(NOAUTH) => {
            let credential =
                password_for(proc, process_id, systemctl).map_err(Shortfall::Withheld)?;
            authenticate(connection, &credential)?;
            connection.ask(&INFO_SERVER)?
        }
        _ => first,
    };

    Ok(InfoServer::parse(&text_of(&INFO_SERVER, answered)?)?)
}

/// The TCP sockets on other ports that were not tried after the chosen one failed, and why not.
///
/// Measured, a plain client on a TLS port is a line in the server's log at its default level, and
/// the kernel's tables cannot say which port is which, so nothing else is tried; the document says
/// what was left instead, so a reader can tell a TLS port from a dead server.
fn untried_note(target: &DialTarget, listening: &[Listener]) -> String {
    let DialTarget::Tcp(address) = target else {
        return String::new();
    };

    let untried: Vec<String> = listening
        .iter()
        .filter(|listener| {
            matches!(listener, Listener::Inet { port, .. } if port.as_u16() != address.port())
        })
        .map(Listener::to_string)
        .collect();

    match untried.is_empty() {
        true => String::new(),
        false => format!(
            "; it also listens on {}, not tried, because that may be its TLS port and a plain \
             client there is a line in the server's log",
            untried.join(", ")
        ),
    }
}

/// One `AUTH`, never repeated.
///
/// **A refusal is not retried with anything else.** Each is an entry in the server's `ACL LOG`,
/// and the only password rastro has is the one the server was started with; a refusal means the
/// running server and its start disagree, which is itself the finding.
fn authenticate(connection: &mut RespConnection, credential: &Credential) -> Result<(), Shortfall> {
    match connection.ask(&["AUTH", credential.password.as_str()])? {
        Reply::Simple(ok) if ok == "OK" => Ok(()),
        _ => Err(Shortfall::Withheld(format!(
            "the server refused the password {} gives the default account, so it has been \
             changed since the server started",
            credential.origin
        ))),
    }
}

/// A command's answer, with the server's refusals said in words.
fn reply_to(connection: &mut RespConnection, command: &[&str]) -> Result<Reply, Shortfall> {
    let reply = connection.ask(command)?;

    refusal_of(command, reply)
}

/// A reply, or the refusal it carries said in words.
fn refusal_of(command: &[&str], reply: Reply) -> Result<Reply, Shortfall> {
    let name = command.join(" ");

    match reply {
        Reply::Error(message) if message.starts_with(NOAUTH) => Err(Shortfall::Withheld(
            "the server requires a password, and none was given".to_owned(),
        )),
        Reply::Error(message) => Err(Shortfall::Withheld(format!(
            "the server refused {name}: {}",
            shortened(&message)
        ))),
        reply => Ok(reply),
    }
}

/// A command whose answer is text.
fn text_reply(connection: &mut RespConnection, command: &[&str]) -> Result<String, Shortfall> {
    let reply = connection.ask(command)?;

    text_of(command, reply)
}

/// A reply that should be text.
fn text_of(command: &[&str], reply: Reply) -> Result<String, Shortfall> {
    match refusal_of(command, reply)? {
        Reply::Bulk(text) => Ok(text),
        other => Err(Shortfall::Failed(format!(
            "the server answered {} with {} rather than text",
            command.join(" "),
            other.kind()
        ))),
    }
}
