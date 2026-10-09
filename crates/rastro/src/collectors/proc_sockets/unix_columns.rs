//! How `/proc/net/unix` lays out a row: fixed columns, then a path that is the rest of it.
//!
//! ```text
//! Num       RefCount Protocol Flags    Type St Inode Path
//! 000000004bfa98b1: 00000002 00000000 00010000 0001 01 11955 /run/systemd/journal/stdout
//! ```
//!
//! Shared, for the reason [`kernel_address`](super::kernel_address) is: `sockets` and
//! `redis` both need the path a unix socket is bound to.

/// How many whitespace-separated columns come before the path.
const LEADING_COLUMNS: usize = 7;

/// One row split into its columns and its path.
pub struct UnixColumns<'a> {
    pub fields: Vec<&'a str>,
    pub path: &'a str,
}

/// Takes the fixed columns off the front and leaves the path untouched, or nothing where the
/// row has too few columns.
///
/// **Not `split_whitespace` over the whole line.** A unix socket path may legally contain a
/// space, and splitting the path into columns would silently truncate it at the first one.
pub fn unix_columns(line: &str) -> Option<UnixColumns<'_>> {
    let mut fields = Vec::with_capacity(LEADING_COLUMNS);
    let mut rest = line;

    for _ in 0..LEADING_COLUMNS {
        let start = rest.find(|character: char| !character.is_whitespace())?;
        rest = &rest[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }

    Some(UnixColumns {
        fields,
        path: rest.trim_start(),
    })
}
