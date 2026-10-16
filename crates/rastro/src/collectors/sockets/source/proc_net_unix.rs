//! The `/proc/net/unix` table.
//!
//! ```text
//! Num       RefCount Protocol Flags    Type St Inode Path
//! 000000004bfa98b1: 00000002 00000000 00010000 0001 01 11955 /run/systemd/journal/stdout
//! ```
//!
//! Everything peculiar to this format lives here: a header line, a flags word carrying
//! `SO_ACCEPTCON`, a numeric socket type, and a path that is the rest of the line rather
//! than a column.

use rastro_collector::CollectionError;

use super::socket_row::SocketRow;
use crate::collectors::sockets::model::SocketAddress;
use crate::collectors::sockets::value_objects::{SocketKind, SocketPath, SocketState};

/// How many whitespace-separated columns come before the path.
const LEADING_COLUMNS: usize = 7;

/// Which of those holds the flags word.
const FLAGS: usize = 3;

/// Which holds the socket type.
const KIND: usize = 4;

/// Which holds the connection state.
const STATE: usize = 5;

/// Which holds the inode that leads to the holding process.
const INODE: usize = 6;

/// The state of a socket that is not connected to a peer.
///
/// **This one value is the whole listener filter**, and it was derived by comparing every
/// row against `ss -l -x` on the development box rather than reasoned about: 99 rows in
/// `/proc/net/unix`, 15 reported by `ss`, and the 15 are exactly the rows in this state.
/// Filtering on `SO_ACCEPTCON` instead would drop the bound datagram sockets, and
/// filtering on having a path would report a connected client twice under its server's
/// name.
const UNCONNECTED: &str = "01";

/// The flag the kernel sets on a socket that accepts connections.
const ACCEPT_CONNECTIONS: u32 = 0x0001_0000;

/// A word of the header, used to tell it from a row rather than counting lines.
const HEADER_MARKER: &str = "RefCount";

/// Translates the table into rows, keeping only the sockets the box is offering.
pub fn parse(text: &str) -> Result<Vec<SocketRow>, CollectionError> {
    let mut rows = Vec::new();

    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.contains(HEADER_MARKER) {
            continue;
        }

        let Some(row) = split_off_path(line) else {
            return Err(CollectionError::new(format!(
                "{line:?} has too few columns, so the table was misread"
            )));
        };

        if row.fields[STATE] != UNCONNECTED {
            continue;
        }

        // **An unnamed socket here is ordinary, not malformed.** `unix_seq_show` prints
        // this state for any socket a process holds that is not established, so one created
        // and not yet bound appears with no name at all. It is not a listener and has no
        // address to report, and refusing it would fail the whole facet whenever a process
        // happened to sit between `socket()` and `bind()`.
        if row.path.is_empty() {
            continue;
        }

        rows.push(SocketRow {
            kind: SocketKind::new(kind_of(row.fields[KIND])?)?,
            state: SocketState::new(state_of(row.fields[FLAGS])?)?,
            address: SocketAddress::Local {
                path: SocketPath::new(row.path)?,
            },
            inode: row.fields[INODE].parse::<u64>().map_err(|error| {
                CollectionError::new(format!(
                    "{:?} is not a socket inode: {error}",
                    row.fields[INODE]
                ))
            })?,
        });
    }

    Ok(rows)
}

/// One row split into its columns and its path.
struct UnixRow<'a> {
    fields: Vec<&'a str>,
    path: &'a str,
}

/// Takes the fixed columns off the front and leaves the path untouched.
///
/// **Not `split_whitespace` over the whole line.** A unix socket path may legally contain a
/// space, and splitting the path into columns would silently truncate it at the first one.
fn split_off_path(line: &str) -> Option<UnixRow<'_>> {
    let mut fields = Vec::with_capacity(LEADING_COLUMNS);
    let mut rest = line;

    for _ in 0..LEADING_COLUMNS {
        let start = rest.find(|character: char| !character.is_whitespace())?;
        rest = &rest[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }

    Some(UnixRow {
        fields,
        path: rest.trim_start(),
    })
}

/// The word for a socket type, in the vocabulary `ss` established.
fn kind_of(field: &str) -> Result<&'static str, CollectionError> {
    match field {
        "0001" => Ok("u_str"),
        "0002" => Ok("u_dgr"),
        "0005" => Ok("u_seq"),
        // A cannot-happen rather than an unfamiliar word: the unix family has had these
        // three types for the life of the interface, so a fourth means the row was misread.
        other => Err(CollectionError::new(format!(
            "{other:?} is not a unix socket type, so the table was misread"
        ))),
    }
}

/// Whether a socket accepts connections or merely holds a name.
///
/// A stream socket sets `SO_ACCEPTCON` when it listens; a datagram socket never does and is
/// still a name the box is offering, which `ss` calls `UNCONN`.
fn state_of(field: &str) -> Result<&'static str, CollectionError> {
    let flags = u32::from_str_radix(field, 16).map_err(|error| {
        CollectionError::new(format!("{field:?} is not a socket flags word: {error}"))
    })?;

    match flags & ACCEPT_CONNECTIONS {
        0 => Ok("UNCONN"),
        _ => Ok("LISTEN"),
    }
}
