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

use std::collections::BTreeSet;
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
        let process = proc.join(process_id.to_string());
        let held = held_socket_inodes(&process.join("fd"))?;

        let mut listeners = BTreeSet::new();
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

            listeners.extend(rows.into_iter().filter_map(|row| match row.address {
                SocketAddress::Inet { host, port } if held.contains(&row.inode) => {
                    Some(Self { port, host })
                }
                _ => None,
            }));
        }

        match read_a_table {
            true => Ok(listeners.into_iter().collect()),
            false => Err(Unread::new(
                "neither net/tcp nor net/tcp6 could be read in the node's namespace",
            )),
        }
    }
}

fn held_socket_inodes(descriptors: &Path) -> Result<BTreeSet<u64>, Unread> {
    let entries = fs::read_dir(descriptors).map_err(|error| {
        Unread::new(format!(
            "the node's descriptors could not be listed: {error}"
        ))
    })?;

    Ok(entries
        .flatten()
        .filter_map(|entry| {
            let target = fs::read_link(entry.path()).ok()?;
            target
                .to_str()?
                .strip_prefix(SOCKET_PREFIX)?
                .strip_suffix(SOCKET_SUFFIX)?
                .parse()
                .ok()
        })
        .collect())
}
