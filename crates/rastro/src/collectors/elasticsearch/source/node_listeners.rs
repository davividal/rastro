//! The sockets a node is listening on, from its own descriptors and its own namespace's tables.
//!
//! `/proc/<pid>/net` is the table of the namespace the process is in, so for a node in a
//! container it is the only table its listeners appear in, and for a node on the host it is
//! the host's. Reading the node's own copy is what makes the port found here and the namespace
//! it is dialled in the same one by construction.
//!
//! The table names no process, so the node's own descriptors say which rows are its: every
//! descriptor holding a socket reads as `socket:[<inode>]`. Only this one process is walked,
//! unlike the box-wide pass the `sockets` facet makes.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use crate::collectors::elasticsearch::value_objects::Unread;
use crate::collectors::inet::{InetHost, PortNumber};
use crate::collectors::sockets::{InetTable, SocketAddress, proc_net_inet};

/// What a descriptor holding a socket reads as, around the inode.
const SOCKET_PREFIX: &str = "socket:[";
const SOCKET_SUFFIX: &str = "]";

/// The two tables a TCP listener can be in.
const TABLES: [(InetTable, &str); 2] = [(InetTable::Tcp, "tcp"), (InetTable::Tcp6, "tcp6")];

/// One socket a node holds in the listening state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeListener {
    pub port: PortNumber,
    pub host: InetHost,
}

impl NodeListener {
    /// The node's listeners through a process table the caller names, ordered by port and host.
    ///
    /// **Every failure is a refusal.** A node whose descriptors cannot be listed, the ordinary
    /// case for an unprivileged run, is not a node listening on nothing: it is one whose port
    /// rastro does not know and so may not dial.
    pub fn read_in(proc: &Path, process_id: u32) -> Result<Vec<Self>, Unread> {
        let listeners: BTreeSet<Self> = listening_in(proc, process_id)?
            .into_iter()
            .map(|held| held.listener)
            .collect();
        Ok(listeners.into_iter().collect())
    }
}

/// The descriptor a node holds a listening socket by, and the socket's inode, so that whether it
/// still does is one look rather than a walk of every descriptor and every socket in its
/// namespace, found by review: that walk ran before each request, outside its deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldSocket {
    descriptor: OsString,
    inode: u64,
}

impl HeldSocket {
    /// Every socket the node listens on `port` by, found once, when the node is.
    ///
    /// **Every one**, found by review: a node bound on two addresses holds two sockets on one
    /// port, and watching whichever came first let the one dialled close unnoticed.
    pub fn all_on_port_in(proc: &Path, process_id: u32, port: u16) -> Result<Vec<Self>, Unread> {
        let sockets: Vec<Self> = listening_in(proc, process_id)?
            .into_iter()
            .filter(|held| held.listener.port.as_u16() == port)
            .map(|held| held.socket)
            .collect();
        match sockets.is_empty() {
            true => Err(Unread::new(format!(
                "the node holds no listener on port {port}"
            ))),
            false => Ok(sockets),
        }
    }

    /// Whether the node's descriptor still names the same socket: closed, or reused for another
    /// file, it does not.
    pub fn still_held_in(&self, proc: &Path, process_id: u32) -> bool {
        let link = proc
            .join(process_id.to_string())
            .join("fd")
            .join(&self.descriptor);
        fs::read_link(link)
            .ok()
            .and_then(|target| socket_inode(&target))
            == Some(self.inode)
    }
}

/// One listening socket of the node's, with the descriptor it holds it by.
struct Held {
    listener: NodeListener,
    socket: HeldSocket,
}

fn listening_in(proc: &Path, process_id: u32) -> Result<Vec<Held>, Unread> {
    let process = proc.join(process_id.to_string());
    let descriptors = held_sockets(&process.join("fd"))?;

    let mut listening = Vec::new();
    let mut read_a_table = false;

    for (table, name) in TABLES {
        let Ok(text) = fs::read_to_string(process.join("net").join(name)) else {
            continue;
        };
        read_a_table = true;

        let rows = proc_net_inet::parse(table, &text).map_err(|error| {
            Unread::new(format!(
                "the node's net/{name} could not be parsed: {error}"
            ))
        })?;

        listening.extend(rows.into_iter().filter_map(|row| match row.address {
            SocketAddress::Inet { host, port } => {
                descriptors.get(&row.inode).map(|descriptor| Held {
                    listener: NodeListener { port, host },
                    socket: HeldSocket {
                        descriptor: descriptor.clone(),
                        inode: row.inode,
                    },
                })
            }
            _ => None,
        }));
    }

    match read_a_table {
        true => Ok(listening),
        false => Err(Unread::new(
            "neither net/tcp nor net/tcp6 could be read in the node's namespace",
        )),
    }
}

/// Each socket the node holds, by inode, with the descriptor it holds it by.
fn held_sockets(descriptors: &Path) -> Result<BTreeMap<u64, OsString>, Unread> {
    let entries = fs::read_dir(descriptors).map_err(|error| {
        Unread::new(format!(
            "the node's descriptors could not be listed: {error}"
        ))
    })?;

    Ok(entries
        .flatten()
        .filter_map(|entry| {
            let inode = socket_inode(&fs::read_link(entry.path()).ok()?)?;
            Some((inode, entry.file_name()))
        })
        .collect())
}

/// The inode a descriptor link names, where it names a socket: `socket:[<inode>]`.
fn socket_inode(target: &Path) -> Option<u64> {
    target
        .to_str()?
        .strip_prefix(SOCKET_PREFIX)?
        .strip_suffix(SOCKET_SUFFIX)?
        .parse()
        .ok()
}
