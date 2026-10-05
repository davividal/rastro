#![allow(dead_code)]

//! A process's start and its account in a fixture `/proc`.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::fs_tree::write;

/// Gives `pid` a start at tick zero and `parent` as its parent, and runs it as the account that
/// owns the fixture, as the files a test makes for it are.
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
    let fixture = std::fs::metadata(proc).expect("the fixture's /proc");
    let (uid, gid) = (fixture.uid(), fixture.gid());
    write(
        proc,
        &format!("{pid}/status"),
        &format!(
            "Name:\tjava\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nGid:\t{gid}\t{gid}\t{gid}\t{gid}\nGroups:\t{gid}\n"
        ),
    );
}
