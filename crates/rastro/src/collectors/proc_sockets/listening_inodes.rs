//! Which sockets are being offered on one port, from the inet tables.
//!
//! ```text
//!   sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
//!    0: 00000000:6448 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 787924 1 ...
//! ```
//!
//! Two columns and a state, which is all this question needs: the local port, the listening
//! state, and the inode that leads to the holding process. Three things a reader of this
//! table gets wrong if it is careless, and each of them is a test:
//!
//! - **the remote column also holds a port**, so a connection *to* the port would be counted
//!   as an offer of it;
//! - **an established connection has the listener's port as its own local port**, so the
//!   state filter is what separates the socket being offered from the clients using it;
//! - **a dual-stack wildcard listener appears only in `tcp6`**, so a reader of `tcp` alone
//!   reports the port as unheld.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// The two tables a TCP port can be offered in.
const TABLES: [&str; 2] = ["tcp", "tcp6"];

/// The state a socket the box is offering is in, as the table spells it.
const LISTENING: &str = "0A";

/// How many columns must be present before the inode can be trusted.
const LEADING_COLUMNS: usize = 10;

/// Which column holds the socket's local end, the connection state, and the inode.
const LOCAL_ADDRESS: usize = 1;
const STATE: usize = 3;
const INODE: usize = 9;

/// A word of the header, used to tell it from a row rather than counting lines.
const HEADER_MARKER: &str = "local_address";

/// The inodes of the sockets being offered on `port`, or nothing where no table could be read.
///
/// **The two answers are different and the caller needs both.** A kernel with IPv6 disabled
/// has no `tcp6` and a readable `tcp` that simply does not carry the port: that is an answer,
/// and it means nothing is offering it. A `/proc/net` that could not be read at all answers
/// nothing, and a caller that treated the two alike would report a service as absent on the
/// strength of a read it never managed.
pub fn listening_inodes(net: &Path, port: u16) -> Option<BTreeSet<u64>> {
    let mut inodes = BTreeSet::new();
    let mut read_a_table = false;

    for table in TABLES {
        let Ok(text) = fs::read_to_string(net.join(table)) else {
            continue;
        };
        read_a_table = true;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.contains(HEADER_MARKER) {
                continue;
            }

            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() < LEADING_COLUMNS || columns[STATE] != LISTENING {
                continue;
            }

            if local_port_of(columns[LOCAL_ADDRESS]) != Some(port) {
                continue;
            }

            if let Ok(inode) = columns[INODE].parse::<u64>() {
                inodes.insert(inode);
            }
        }
    }

    match read_a_table {
        true => Some(inodes),
        false => None,
    }
}

/// The port of an `<address>:<port>` column, where the port is hexadecimal.
///
/// The address half is not decoded at all. Which interface a socket is bound to is a question
/// about what the box offers, and it is answered properly by the `sockets` facet; here the
/// only question is which process to ask about a service, and a service offered on a port is
/// offered by whoever holds that port whichever address it is bound to.
fn local_port_of(column: &str) -> Option<u16> {
    let (_, port) = column.rsplit_once(':')?;

    u16::from_str_radix(port, 16).ok()
}
