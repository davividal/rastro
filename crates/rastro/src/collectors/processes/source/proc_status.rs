//! The `/proc/<pid>/status` interface.
//!
//! `Name:\tsystemd`, one field per line, tab after the colon. Chosen over `/proc/<pid>/stat`
//! because `stat` puts the process name in parentheses and a name may itself contain
//! parentheses and spaces, so every field after it needs the *last* `)` to be located first.
//! This file has no such trap.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

/// The fields rastro reads out of the file. It carries about fifty more, almost all of them
/// memory and signal accounting that moves continuously.
pub const NAME: &str = "Name";
pub const STATE: &str = "State";
pub const PARENT: &str = "PPid";
pub const USER: &str = "Uid";
pub const GROUP: &str = "Gid";
pub const THREADS: &str = "Threads";

/// Reads the file into its fields.
///
/// A map rather than a struct, because the caller wants six of fifty-odd lines and the file
/// is a flat key-value list. Fields the kernel does not write on a given release are simply
/// absent, which the caller then reports as a misread rather than guessing.
pub fn parse(status: &str) -> BTreeMap<String, String> {
    status
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

/// One field, or a failure naming it.
pub fn field<'a>(
    fields: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, CollectionError> {
    fields
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| CollectionError::new(format!("a process reported no {name:?} field")))
}

/// The first of the four space-separated ids on a `Uid:` or `Gid:` line.
///
/// The kernel writes real, effective, saved-set and filesystem ids in that order. The
/// **real** id is taken, because it is the account the process belongs to rather than the
/// one it is currently acting as: a process that dropped privileges keeps its real id, and
/// that is the honest answer to "whose process is this".
pub fn real_id(line: &str) -> Result<u32, CollectionError> {
    let real = line.split_whitespace().next().ok_or_else(|| {
        CollectionError::new(format!("{line:?} carries no ids, so the line was misread"))
    })?;

    real.parse::<u32>()
        .map_err(|_| CollectionError::new(format!("{real:?} is not a user or group id")))
}
