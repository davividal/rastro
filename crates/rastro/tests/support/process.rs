#![allow(dead_code)]

//! A process's start time in a fixture `/proc`, which the settings read compares the node's file
//! against.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::fs_tree::write;

/// Gives `pid` a start `seconds_ago` before now, and `parent` as its parent.
///
/// The start is `btime` from `/proc/stat` plus field 22 of the process's own `stat`, in clock
/// ticks; this puts the whole offset in `btime` and the ticks at zero, so it holds whatever the
/// box's tick rate. A `/proc/stat` already written is replaced, so one fixture has one boot.
pub fn started(proc: &Path, pid: &str, parent: &str, seconds_ago: u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs();
    write(
        proc,
        "stat",
        &format!("cpu  0 0 0 0\nbtime {}\n", now - seconds_ago),
    );
    // Fields 3 to 21 after the name, then the start time, then a few more, as the kernel writes.
    let middle = vec!["0"; 17].join(" ");
    write(
        proc,
        &format!("{pid}/stat"),
        &format!("{pid} (java) S {parent} {middle} 0 0 0\n"),
    );
}
