//! The sockets the box is offering, each with its address and the inode that leads to its holder.
//!
//! ```text
//!   sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
//!    0: 00000000:6448 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787924 1 ...
//! ```
//!
//! Three things a reader of the inet tables gets wrong if it is careless, and each of them is a
//! test:
//!
//! - **the remote column also holds a port**, so a connection *to* the port would be counted
//!   as an offer of it;
//! - **an established connection has the listener's port as its own local port**, so the
//!   state filter is what separates the socket being offered from the clients using it;
//! - **a dual-stack wildcard listener appears only in `tcp6`**, so a reader of `tcp` alone
//!   reports the port as unheld.
//!
//! **A row that does not parse is skipped rather than failing the read.** The callers here ask
//! which process to talk to, and one odd row elsewhere in the table says nothing about that. The
//! `sockets` facet, which reports every row, is the reader that refuses a malformed one.

use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use super::kernel_address::{ipv4_of, ipv6_of};
use super::unix_columns::unix_columns;

/// The two tables a TCP port can be offered in, and whether each writes IPv6 addresses.
const INET_TABLES: [(&str, bool); 2] = [("tcp", false), ("tcp6", true)];

/// The table unix sockets are published in.
const UNIX_TABLE: &str = "unix";

/// The state a TCP socket the box is offering is in, as the table spells it.
const LISTENING: &str = "0A";

/// How many columns must be present before an inet row's inode can be trusted.
const LEADING_COLUMNS: usize = 10;

/// Which inet column holds the socket's local end, the connection state, and the inode.
const LOCAL_ADDRESS: usize = 1;
const STATE: usize = 3;
const INODE: usize = 9;

/// Which unix columns hold the flags, the socket type, the state and the inode.
const UNIX_FLAGS: usize = 3;
const UNIX_KIND: usize = 4;
const UNIX_STATE: usize = 5;
const UNIX_INODE: usize = 6;

/// A unix socket that accepts connections: `SO_ACCEPTCON` set, a stream, and unconnected.
const ACCEPT_CONNECTIONS: u32 = 0x0001_0000;
const STREAM: &str = "0001";
const UNCONNECTED: &str = "01";

/// Words of the two headers, used to tell them from a row rather than counting lines.
const INET_HEADER: &str = "local_address";
const UNIX_HEADER: &str = "RefCount";

/// A TCP socket the box is offering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InetListener {
    pub address: SocketAddr,
    pub inode: u64,
}

/// A unix stream socket the box is offering, under the name it is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnixListener {
    pub path: String,
    pub inode: u64,
}

/// Every TCP socket being offered, or nothing where neither table could be read.
///
/// **The two answers are different and the caller needs both.** A kernel with IPv6 disabled
/// has no `tcp6` and a readable `tcp`: that is an answer. A `/proc/net` that could not be read
/// at all answers nothing, and a caller that treated the two alike would report a service as
/// absent on the strength of a read it never managed.
pub fn inet_listeners(net: &Path) -> Option<Vec<InetListener>> {
    let mut listeners = Vec::new();
    let mut read_a_table = false;

    for (table, is_ipv6) in INET_TABLES {
        let Ok(text) = fs::read_to_string(net.join(table)) else {
            continue;
        };
        read_a_table = true;

        listeners.extend(
            text.lines()
                .filter(|line| !line.contains(INET_HEADER))
                .filter_map(|line| inet_listener_of(line, is_ipv6)),
        );
    }

    read_a_table.then_some(listeners)
}

/// Every unix stream socket accepting connections under a name, or nothing where the table
/// could not be read.
pub fn unix_listeners(net: &Path) -> Option<Vec<UnixListener>> {
    let text = fs::read_to_string(net.join(UNIX_TABLE)).ok()?;

    Some(
        text.lines()
            .filter(|line| !line.contains(UNIX_HEADER))
            .filter_map(unix_listener_of)
            .collect(),
    )
}

fn inet_listener_of(line: &str, is_ipv6: bool) -> Option<InetListener> {
    let columns: Vec<&str> = line.split_whitespace().collect();
    if columns.len() < LEADING_COLUMNS || columns[STATE] != LISTENING {
        return None;
    }

    let (host, port) = columns[LOCAL_ADDRESS].split_once(':')?;
    let host: IpAddr = match is_ipv6 {
        true => ipv6_of(host).ok()?.into(),
        false => ipv4_of(host).ok()?.into(),
    };

    Some(InetListener {
        address: SocketAddr::new(host, u16::from_str_radix(port, 16).ok()?),
        inode: columns[INODE].parse().ok()?,
    })
}

fn unix_listener_of(line: &str) -> Option<UnixListener> {
    // `lines` has already taken the terminator; a path may itself end in a space.
    let row = unix_columns(line)?;
    let flags = u32::from_str_radix(row.fields[UNIX_FLAGS], 16).ok()?;

    let accepts = flags & ACCEPT_CONNECTIONS != 0
        && row.fields[UNIX_KIND] == STREAM
        && row.fields[UNIX_STATE] == UNCONNECTED;
    if !accepts || row.path.is_empty() {
        return None;
    }

    Some(UnixListener {
        path: row.path.to_owned(),
        inode: row.fields[UNIX_INODE].parse().ok()?,
    })
}
