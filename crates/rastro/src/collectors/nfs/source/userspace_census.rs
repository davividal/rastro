//! The `/proc` interface: whether NFS is served from userspace.

use std::fs;
use std::path::Path;

use crate::collectors::processes::source::proc_processes::departed;

/// The program name nfs-ganesha runs under.
const GANESHA: &str = "ganesha.nfsd";

/// Whether a userspace NFS server runs, as far as the process table showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserspaceServer {
    NotRunning,
    Running {
        process_id: u32,
    },

    /// Some process's program was refused rather than gone, so a server could be among them.
    Unseen(String),
}

/// The first userspace server in the process table, by `comm`, which nothing in userspace
/// rewrites. Never fails: an unreadable `/proc` is every process unseen.
pub fn userspace_server(proc: &Path) -> UserspaceServer {
    let Ok(entries) = fs::read_dir(proc) else {
        return UserspaceServer::Unseen(format!("could not list {}", proc.display()));
    };

    let mut process_ids: Vec<u32> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect();
    process_ids.sort_unstable();

    let mut unseen = None;
    for process_id in process_ids {
        let comm = proc.join(process_id.to_string()).join("comm");
        match fs::read_to_string(&comm) {
            Ok(program) if program.trim_end() == GANESHA => {
                return UserspaceServer::Running { process_id };
            }
            Ok(_) => {}
            Err(error) if departed(&error) => {}
            Err(error) => {
                unseen.get_or_insert_with(|| format!("could not read {}: {error}", comm.display()));
            }
        }
    }

    unseen.map_or(UserspaceServer::NotRunning, UserspaceServer::Unseen)
}
