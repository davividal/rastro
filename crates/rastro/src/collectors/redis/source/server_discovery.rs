//! The servers on the box, from `/proc` alone: what each is keyed by and where it can be reached.
//!
//! **Only a socket the server was seen holding is ever reached.** The kernel says which sockets
//! are listening and which process holds each, so the address rastro connects to is one the
//! server itself bound, never a default and never a port some other process owns. That is what
//! keeps a connection from being speculative: nothing is dialled to find out whether redis is
//! there.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use crate::collectors::inet::{InetHost, PortNumber};
use crate::collectors::proc_sockets::{
    InetListener, UnixListener, inet_listeners, sockets_held_by, unix_listeners,
};
use crate::collectors::redis::value_objects::{Listener, ServerKind};

use super::resident_servers::{ResidentServer, resident_servers};

/// Where the kernel publishes its socket tables under the process table.
const NET: &str = "net";

/// Where a server can be connected to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialTarget {
    Unix(PathBuf),
    Tcp(SocketAddr),
}

impl fmt::Display for DialTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DialTarget::Unix(path) => write!(formatter, "{}", path.display()),
            DialTarget::Tcp(address) => write!(formatter, "{address}"),
        }
    }
}

/// One server process, before anything has been asked of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredServer {
    /// What the facet keys the instance by.
    ///
    /// **The TCP port, else the unix socket's path, else the process title**, and the lowest
    /// address on the port where two servers share it. A port rather
    /// than an address, so a change of `bind`, which is exactly what the field host needed seeing,
    /// is a change to one instance rather than one vanishing and another appearing. The title is
    /// the fallback for a server whose sockets cannot be attributed: it is what redis calls
    /// itself, `/usr/bin/redis-server 127.0.0.1:6379` on Debian, and anybody may read it.
    pub key: String,

    pub kind: ServerKind,

    /// The process, for the reads that need it and never for the document: it changes on every
    /// restart of a box nobody touched.
    pub process_id: u32,

    /// Every socket the server holds, sorted.
    pub listeners: Vec<Listener>,

    /// The socket rastro would connect to, or why there is none.
    pub reach: Result<DialTarget, String>,

    /// The key where another server shares the port: the lowest address on it, which a restart
    /// keeps, where numbering in pid order would swap the two.
    shared_port_key: Option<String>,
}

/// Every server process on the box, in pid order, each under a key no other one has.
///
/// Never fails as a whole: what cannot be read about one server is its own `reach`, so a box
/// with two servers where one is unreadable still reports the other.
pub fn discover(proc: &Path) -> Vec<DiscoveredServer> {
    let net = proc.join(NET);
    let inet = inet_listeners(&net);
    let unix = unix_listeners(&net);

    let mut discovered: Vec<DiscoveredServer> = resident_servers(proc)
        .into_iter()
        .map(|server| discovered(proc, &server, inet.as_deref(), unix.as_deref()))
        .collect();

    let mut keyed = BTreeMap::<String, usize>::new();
    for server in &discovered {
        *keyed.entry(server.key.clone()).or_default() += 1;
    }
    for server in &mut discovered {
        if keyed[&server.key] > 1
            && let Some(address) = server.shared_port_key.clone()
        {
            server.key = address;
        }
    }

    // Two unattributable servers can share a title, and a map keyed by it would keep one of
    // them silently. Numbered in pid order, which only this degenerate case ever sees.
    let mut seen = BTreeSet::new();
    for server in &mut discovered {
        let mut candidate = server.key.clone();
        let mut ordinal = 1;
        while !seen.insert(candidate.clone()) {
            ordinal += 1;
            candidate = format!("{} #{ordinal}", server.key);
        }
        server.key = candidate;
    }

    discovered
}

fn discovered(
    proc: &Path,
    server: &ResidentServer,
    inet: Option<&[InetListener]>,
    unix: Option<&[UnixListener]>,
) -> DiscoveredServer {
    let title = title_of(proc, server);
    let unreached = |reason: String| DiscoveredServer {
        key: title.clone(),
        kind: server.kind,
        process_id: server.process_id,
        listeners: Vec::new(),
        reach: Err(reason),
        shared_port_key: None,
    };

    let held = match sockets_held_by(proc, server.process_id) {
        Ok(held) => held,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return unreached("the server exited while it was being read".to_owned());
        }
        Err(error) => {
            return unreached(format!(
                "its file descriptors could not be read ({error}), so which sockets it listens \
                 on is unknown; a run as root can see them"
            ));
        }
    };

    if inet.is_none() && unix.is_none() {
        return unreached(
            "the kernel's socket tables could not be read, so which sockets it listens on is \
             unknown"
                .to_owned(),
        );
    }

    let mut addresses: Vec<SocketAddr> = inet
        .unwrap_or_default()
        .iter()
        .filter(|listener| held.contains(&listener.inode))
        .map(|listener| listener.address)
        .collect();
    let mut paths: Vec<String> = unix
        .unwrap_or_default()
        .iter()
        .filter(|listener| held.contains(&listener.inode))
        .map(|listener| listener.path.clone())
        .collect();
    addresses.sort_unstable();
    addresses.dedup();
    paths.sort_unstable();
    paths.dedup();

    let Some(reach) = reach_of(&addresses, &paths, titled_port(&title)) else {
        return unreached("it listens on nothing rastro could connect to".to_owned());
    };

    let lowest_port = addresses.iter().map(SocketAddr::port).min();
    let key = match (lowest_port, paths.first()) {
        (Some(port), _) => port.to_string(),
        (None, Some(path)) => path.clone(),
        (None, None) => title.clone(),
    };
    let shared_port_key = addresses
        .iter()
        .find(|address| Some(address.port()) == lowest_port)
        .map(SocketAddr::to_string);

    let mut listeners: Vec<Listener> = addresses
        .iter()
        .filter_map(|address| {
            Some(Listener::Inet {
                host: InetHost::new(address.ip().to_string()).ok()?,
                port: PortNumber::from(address.port()),
            })
        })
        .chain(paths.into_iter().map(|path| Listener::Local { path }))
        .collect();
    listeners.sort_unstable();

    DiscoveredServer {
        key,
        kind: server.kind,
        process_id: server.process_id,
        listeners,
        reach: Ok(reach),
        shared_port_key,
    }
}

/// The one socket to connect to, in order of how local it is.
///
/// A unix socket first, since it involves no network stack at all, and redis serves no TLS on
/// one; then loopback; then a wildcard, reached on the loopback address of its own family; then
/// an address of the box's own, which the kernel delivers locally without it ever reaching a
/// wire. Each list arrives sorted, so the choice is the same on every run.
///
/// **Among TCP sockets, the port the title names comes first.** A plain `port` and a `tls-port`
/// look the same in the kernel's tables, measured, and a plain client on the TLS one is a line in
/// the server's log at its default level, also measured, so trying one and then the other would
/// write to the box. redis's title shows the plain port whenever there is one.
fn reach_of(
    addresses: &[SocketAddr],
    paths: &[String],
    titled_port: Option<u16>,
) -> Option<DialTarget> {
    if let Some(path) = paths.first() {
        return Some(DialTarget::Unix(PathBuf::from(path)));
    }

    let on_titled_port: Vec<SocketAddr> = addresses
        .iter()
        .filter(|address| Some(address.port()) == titled_port)
        .copied()
        .collect();
    let addresses = match on_titled_port.is_empty() {
        true => addresses,
        false => on_titled_port.as_slice(),
    };

    let loopback = addresses.iter().find(|address| address.ip().is_loopback());
    let wildcard = addresses
        .iter()
        .find(|address| address.ip().is_unspecified());
    let own = addresses.first();

    let chosen = match (loopback, wildcard) {
        (Some(address), _) => *address,
        (None, Some(address)) => SocketAddr::new(loopback_of(address.ip()), address.port()),
        (None, None) => *own?,
    };

    Some(DialTarget::Tcp(chosen))
}

/// The port in the title's `{listen-addr}`, `127.0.0.1:6379` or `*:6380` after the program, or
/// nothing where the template leaves it out or names a unix socket instead.
///
/// Measured: the plain port whenever the server has one, and the TLS port only where it has no
/// other, so the port it names is never the TLS one while a plain one exists.
fn titled_port(title: &str) -> Option<u16> {
    let listen_address = title.split_whitespace().nth(1)?;

    listen_address.rsplit_once(':')?.1.parse().ok()
}

fn loopback_of(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
    }
}

/// The process title, as redis rewrote its argument vector, or the program's name where there
/// is no title to read.
///
/// The rewrite leaves the vector's old space padded with NULs, so the title is what precedes
/// them, with any separators read as the spaces they stand for. A title that is not UTF-8 is
/// not repaired into one, for the reason every other text in the document is not.
fn title_of(proc: &Path, server: &ResidentServer) -> String {
    let cmdline = fs::read(proc.join(server.process_id.to_string()).join("cmdline"))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();

    let title = cmdline
        .split('\0')
        .filter(|argument| !argument.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    match title.is_empty() {
        true => server.kind.program().to_owned(),
        false => title,
    }
}
