//! The `/proc/net/{tcp,tcp6,udp,udp6}` tables.
//!
//! ```text
//!   sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
//!    1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 45643 1 ...
//! ```
//!
//! Everything peculiar to this format lives here: a header line, addresses written as
//! host-order hexadecimal words, a state column whose meaning depends on the protocol, and
//! a trailing set of columns that varies by kernel version and is deliberately not read.

use rastro_collector::CollectionError;

use super::inet_table::InetTable;
use super::socket_row::SocketRow;
use crate::collectors::inet::{InetHost, PortNumber};
use crate::collectors::proc_sockets::{ipv4_of, ipv6_of};
use crate::collectors::sockets::model::SocketAddress;
use crate::collectors::sockets::value_objects::{SocketKind, SocketState};

/// How many columns must be present before the inode can be trusted.
///
/// `sl local_address rem_address st tx:rx tr:when retrnsmt uid timeout inode`. Everything
/// after the inode differs between kernels and is not read, so this is a minimum rather
/// than an exact count.
const LEADING_COLUMNS: usize = 10;

/// Which column holds the socket's local end.
const LOCAL_ADDRESS: usize = 1;

/// Which column holds the connection state.
const STATE: usize = 3;

/// Which column holds the inode that leads to the holding process.
const INODE: usize = 9;

/// A word of the header, used to tell it from a row rather than counting lines.
const HEADER_MARKER: &str = "local_address";

/// Translates one table into rows, keeping only the sockets the box is offering.
pub fn parse(table: InetTable, text: &str) -> Result<Vec<SocketRow>, CollectionError> {
    let mut rows = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.contains(HEADER_MARKER) {
            continue;
        }

        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < LEADING_COLUMNS {
            return Err(CollectionError::new(format!(
                "{line:?} has {} columns, so the table was misread",
                fields.len()
            )));
        }

        // Read before the state is checked, so a malformed row is a failure even when it
        // was going to be skipped. A row rastro cannot parse is a row it cannot promise it
        // would have reported.
        let address = parse_address(table, fields[LOCAL_ADDRESS])?;
        let inode = fields[INODE].parse::<u64>().map_err(|error| {
            CollectionError::new(format!(
                "{:?} is not a socket inode: {error}",
                fields[INODE]
            ))
        })?;

        if fields[STATE] != table.offered_state() {
            continue;
        }

        rows.push(SocketRow {
            kind: SocketKind::new(table.kind())?,
            state: SocketState::new(table.state_word())?,
            address,
            inode,
        });
    }

    Ok(rows)
}

/// Splits `HEX:HEX` into an address and a port.
///
/// The address half has no colon in it whatever the family, because it is raw hexadecimal
/// rather than the printed form, which is the one way this table is easier to read than
/// `ss`'s output.
fn parse_address(table: InetTable, column: &str) -> Result<SocketAddress, CollectionError> {
    let (host, port) = column.split_once(':').ok_or_else(|| {
        CollectionError::new(format!(
            "{column:?} is not an address and port, so the row was misread"
        ))
    })?;

    let host = match table.is_ipv6() {
        true => ipv6_of(host)?.to_string(),
        false => ipv4_of(host)?.to_string(),
    };

    Ok(SocketAddress::Inet {
        host: InetHost::new(host)?,
        port: PortNumber::parse_hexadecimal(port)?,
    })
}
