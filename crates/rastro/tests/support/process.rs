#![allow(dead_code)]

//! A process's start in a fixture `/proc`.

use std::path::Path;

use super::fs_tree::write;

/// Gives `pid` a start at tick zero and `parent` as its parent.
///
/// The start is field 22 of the process's `stat`, which with the pid names the process; nothing
/// reads it as a time, so no `btime` is written.
pub fn started(proc: &Path, pid: &str, parent: &str) {
    // Fields 3 to 21 after the name, then the start time, then a few more, as the kernel writes.
    let middle = vec!["0"; 17].join(" ");
    write(
        proc,
        &format!("{pid}/stat"),
        &format!("{pid} (java) S {parent} {middle} 0 0 0\n"),
    );
}
