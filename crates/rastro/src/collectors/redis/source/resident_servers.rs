//! The `/proc` interface: which server processes are running.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::collectors::redis::value_objects::ServerKind;

/// One running server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentServer {
    pub process_id: u32,
    pub kind: ServerKind,
}

/// The server processes in a process table, and whether some process could not be inspected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResidentCensus {
    /// In ascending pid order.
    pub servers: Vec<ResidentServer>,

    /// Whether some process's program was refused rather than gone, so a server could be among
    /// them.
    ///
    /// Found by review, as elasticsearch found it: under `hidepid=1` an unprivileged run sees a
    /// process's directory and is refused its `comm`, and that was read as no server, so the facet
    /// said `absent` about a box it could not see. `hidepid=2` hides the directory itself, which no
    /// reading can tell from no process.
    pub some_processes_unseen: bool,
}

/// The server processes in a process table, in ascending pid order, and whether any were unseen.
///
/// **By `comm`, never by `cmdline`.** redis overwrites its argument vector with a process title
/// once it has started, `/usr/bin/redis-server 127.0.0.1:6379` on Debian, and the title is an
/// operator-configurable template, so the vector says neither what the program is nor where its
/// configuration came from. `comm` is the kernel's record of the program name and nothing in
/// userspace rewrites it here.
///
/// **Never fails.** An entry that vanished mid-walk is a process that left; an unreadable `/proc`
/// is every process unseen, so the facet cannot call the box empty.
pub fn resident_census(proc: &Path) -> ResidentCensus {
    let Ok(entries) = fs::read_dir(proc) else {
        return ResidentCensus {
            servers: Vec::new(),
            some_processes_unseen: true,
        };
    };

    let mut census = ResidentCensus::default();
    for entry in entries.flatten() {
        let Some(process_id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        match fs::read_to_string(entry.path().join("comm")) {
            Ok(comm) => {
                if let Some(kind) = ServerKind::from_program(comm.trim_end_matches('\n')) {
                    census.servers.push(ResidentServer { process_id, kind });
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => census.some_processes_unseen = true,
        }
    }

    // Directory order is the filesystem's, and a list that moves between two runs is what the
    // document's contract forbids.
    census
        .servers
        .sort_unstable_by_key(|server| server.process_id);

    // A background save or AOF rewrite forks a child that keeps `comm` and closes the listeners,
    // measured on every version; it is the server's work, not a second server.
    let servers: BTreeSet<u32> = census
        .servers
        .iter()
        .map(|server| server.process_id)
        .collect();
    census.servers.retain(|server| {
        parent_of(proc, server.process_id).is_none_or(|parent| !servers.contains(&parent))
    });

    census
}

/// The server processes in a process table, in ascending pid order.
pub fn resident_servers(proc: &Path) -> Vec<ResidentServer> {
    resident_census(proc).servers
}

/// The parent a process's `stat` names, or nothing where it cannot be read.
///
/// Read after the last `)`, because `comm` is in parentheses and may itself hold one.
fn parent_of(proc: &Path, process_id: u32) -> Option<u32> {
    let stat = fs::read_to_string(proc.join(process_id.to_string()).join("stat")).ok()?;
    let (_, after_comm) = stat.rsplit_once(')')?;

    after_comm.split_whitespace().nth(1)?.parse().ok()
}
