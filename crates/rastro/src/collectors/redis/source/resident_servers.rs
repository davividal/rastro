//! The `/proc` interface: which server processes are running.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::collectors::redis::value_objects::ServerKind;

/// One running server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentServer {
    pub process_id: u32,
    pub kind: ServerKind,
}

/// The server processes in a process table, in ascending pid order.
///
/// **By `comm`, never by `cmdline`.** redis overwrites its argument vector with a process title
/// once it has started, `/usr/bin/redis-server 127.0.0.1:6379` on Debian, and the title is an
/// operator-configurable template, so the vector says neither what the program is nor where its
/// configuration came from. `comm` is the kernel's record of the program name and nothing in
/// userspace rewrites it here.
///
/// **Never fails.** An unreadable `/proc` and an entry that vanished mid-walk both mean nothing
/// was found, and an error would put the facet in `error` on a box with no redis at all.
pub fn resident_servers(proc: &Path) -> Vec<ResidentServer> {
    let Ok(entries) = fs::read_dir(proc) else {
        return Vec::new();
    };

    let mut resident: Vec<ResidentServer> = entries
        .flatten()
        .filter_map(|entry| {
            let process_id: u32 = entry.file_name().to_str()?.parse().ok()?;
            let comm = fs::read_to_string(entry.path().join("comm")).ok()?;
            let kind = ServerKind::from_program(comm.trim_end_matches('\n'))?;

            Some(ResidentServer { process_id, kind })
        })
        .collect();

    // Directory order is the filesystem's, and a list that moves between two runs is what the
    // document's contract forbids.
    resident.sort_unstable_by_key(|server| server.process_id);

    // A background save or AOF rewrite forks a child that keeps `comm` and closes the listeners,
    // measured on every version; it is the server's work, not a second server.
    let servers: BTreeSet<u32> = resident.iter().map(|server| server.process_id).collect();
    resident.retain(|server| {
        parent_of(proc, server.process_id).is_none_or(|parent| !servers.contains(&parent))
    });

    resident
}

/// The parent a process's `stat` names, or nothing where it cannot be read.
///
/// Read after the last `)`, because `comm` is in parentheses and may itself hold one.
fn parent_of(proc: &Path, process_id: u32) -> Option<u32> {
    let stat = fs::read_to_string(proc.join(process_id.to_string()).join("stat")).ok()?;
    let (_, after_comm) = stat.rsplit_once(')')?;

    after_comm.split_whitespace().nth(1)?.parse().ok()
}
