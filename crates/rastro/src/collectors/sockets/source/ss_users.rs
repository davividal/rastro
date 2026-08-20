//! The `users:((...))` column `ss -p` appends.
//!
//! `users:(("systemd-journal",pid=3989,fd=5),("systemd",pid=1,fd=117))`
//!
//! Its own module because it is a nested grammar rather than a column: a list of triples
//! inside one whitespace-free token, and the only part of `ss`'s output that is not
//! positional.

use std::collections::BTreeSet;

use rastro_collector::CollectionError;

use crate::collectors::sockets::model::SocketProcess;
use crate::collectors::sockets::value_objects::ProcessName;

const PREFIX: &str = "users:((";
const SUFFIX: &str = "))";
const SEPARATOR: &str = "),(";

const PROCESS_ID: &str = "pid=";
const FILE_DESCRIPTOR: &str = "fd=";

/// Reads the holders of one socket.
///
/// **An absent column is no holders, not a failure.** `ss` omits the column entirely for
/// a socket the kernel holds with no userspace process behind it, and for any socket when
/// rastro is not privileged enough to be told. Both are honestly described by an empty
/// set, and refusing would lose the whole facet over a socket nobody owns.
pub fn parse(column: Option<&str>) -> Result<BTreeSet<SocketProcess>, CollectionError> {
    let Some(column) = column else {
        return Ok(BTreeSet::new());
    };

    let inner = column
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .ok_or_else(|| {
            CollectionError::new(format!(
                "{column:?} is not the process column `ss -p` writes"
            ))
        })?;

    inner.split(SEPARATOR).map(parse_one).collect()
}

/// One `"name",pid=N,fd=M` triple.
///
/// The name is quoted and the numbers are not, and the name is taken by position rather
/// than by unquoting: a process name comes from the kernel's `comm`, which cannot contain
/// a comma or a quote, so the first two commas are always the triple's separators.
fn parse_one(triple: &str) -> Result<SocketProcess, CollectionError> {
    let fields: Vec<&str> = triple.split(',').collect();
    let [name, process_id, file_descriptor] = fields.as_slice() else {
        return Err(CollectionError::new(format!(
            "expected a name, a pid and an fd in {triple:?}, got {} fields",
            fields.len()
        )));
    };

    Ok(SocketProcess {
        name: ProcessName::new(name.trim_matches('"'))?,
        process_id: numbered(process_id, PROCESS_ID)?,
        file_descriptor: numbered(file_descriptor, FILE_DESCRIPTOR)?,
    })
}

fn numbered(field: &str, label: &str) -> Result<i64, CollectionError> {
    let value = field.strip_prefix(label).ok_or_else(|| {
        CollectionError::new(format!("expected {field:?} to begin with {label:?}"))
    })?;

    value
        .parse::<i64>()
        .map_err(|_| CollectionError::new(format!("{value:?} is not a number")))
}
