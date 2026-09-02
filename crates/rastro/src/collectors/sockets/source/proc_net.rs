//! The `/proc` interface to what the host is listening on.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::inet_table::InetTable;
use super::proc_fd::SocketHolders;
use super::socket_row::SocketRow;
use super::{proc_net_inet, proc_net_unix};
use crate::collectors::sockets::model::{ListeningSocket, SocketTable};

/// Where the kernel publishes its network tables.
const PROC_NET: &str = "/proc/net";

/// Where the kernel publishes its processes.
const PROC: &str = "/proc";

/// The table that must be readable for this facet to mean anything.
///
/// A kernel without IPv6 has no `tcp6`, and a container may have neither, but every Linux
/// with a procfs has `unix`, so its absence means rastro is not looking at a procfs.
const UNIX: &str = "unix";

/// The socket tables as a source rastro can read.
///
/// # Why not `ss`
///
/// `ss` is the canonical tool for this facet and it cannot be used, because asking it costs
/// the host a change. `ss -t -u` makes the kernel autoload `udp_diag` and `ss -x` loads
/// `unix_diag`, and those modules stay loaded: a first run on a fresh box leaves two
/// modules behind and a before-and-after pair blames them on whatever change was under
/// test. That was measured on the development box, not inferred. These tables load nothing.
///
/// # What that costs
///
/// One field. `ss` reports the interface a socket is pinned to with `SO_BINDTODEVICE`,
/// printing `127.0.0.53%lo`, and the kernel returns that over the diag netlink socket and
/// nowhere else: no column of `/proc/net/tcp` carries it. It is not recoverable by
/// inference either, which was checked rather than assumed — `127.0.0.53` carries the scope
/// and `127.0.0.54` does not, and neither appears in `ip addr`, so deriving the scope from
/// the address would invent one for the second. The field is therefore gone rather than
/// guessed, and the gap it leaves is a wildcard bind that is really reachable on one
/// interface only.
///
/// The holding process is *not* part of that cost. `/proc/net/*` names no process, but the
/// inode it carries leads to one through `/proc/<pid>/fd`, which resolved every listening
/// socket on the development box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcNet {
    net: PathBuf,
    proc: PathBuf,
}

impl ProcNet {
    /// Finds the interface, or reports that this host does not present one.
    pub fn detect() -> Option<Self> {
        let source = Self::new();

        source.net.join(UNIX).is_file().then_some(source)
    }

    pub fn new() -> Self {
        Self {
            net: PathBuf::from(PROC_NET),
            proc: PathBuf::from(PROC),
        }
    }

    /// The same over paths the caller chose.
    pub fn at(net: impl Into<PathBuf>, proc: impl Into<PathBuf>) -> Self {
        Self {
            net: net.into(),
            proc: proc.into(),
        }
    }

    /// Reads every table and joins each socket to the processes holding it.
    pub fn read(&self) -> Result<SocketTable, CollectionError> {
        let mut rows = Vec::new();

        for (table, file) in InetTable::ALL {
            // A missing table is a kernel without that family, which is state rather than
            // failure: a box with IPv6 disabled has no `tcp6` and is listening on no IPv6
            // socket. An unreadable one that exists is a different matter and fails.
            let path = self.net.join(file);
            if !path.is_file() {
                continue;
            }
            rows.extend(proc_net_inet::parse(table, &read(&path)?)?);
        }

        rows.extend(proc_net_unix::parse(&read(&self.net.join(UNIX))?)?);

        Ok(SocketTable::new(self.attributed(rows)))
    }

    /// Turns rows into sockets by finding who holds each one.
    ///
    /// The holders are read once, after every table, so a socket opened while rastro was
    /// part way through the tables is either in both readings or in neither.
    fn attributed(&self, rows: Vec<SocketRow>) -> Vec<ListeningSocket> {
        let holders = SocketHolders::at(&self.proc);

        rows.into_iter()
            .map(|row| ListeningSocket {
                kind: row.kind,
                state: row.state,
                address: row.address,
                processes: holders.of(row.inode),
            })
            .collect()
    }
}

impl Default for ProcNet {
    fn default() -> Self {
        Self::new()
    }
}

fn read(path: &Path) -> Result<String, CollectionError> {
    fs::read_to_string(path).map_err(|error| {
        CollectionError::new(format!("could not read {}: {error}", path.display()))
    })
}
