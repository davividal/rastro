//! The directory a server writes into, from its working directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::collectors::host_directories::host_directory_views_of;

/// The files a server writes in its directory, under redis's default names: the snapshot, and
/// the directory of append-only files.
///
/// **Its files, not its directory**, decided by the maintainer: a server started by hand from
/// `/root` with `dir ./` keeps its dump in root's home, and sealing the directory would hide the
/// home's keys and scripts to hide one file. Claims are gathered before any server is asked, so a
/// renamed `dbfilename` is not known, and that file is walked like any other.
const SERVER_FILES: [&str; 2] = ["dump.rdb", "appendonlydir"];

/// The host paths of a server's own files, or none where that cannot be told.
///
/// **The working directory is the setting.** redis applies `dir` with `chdir`, at start and on
/// every `CONFIG SET dir`, and `CONFIG GET dir` answers with `getcwd`, so `/proc/<pid>/cwd` is
/// the directory without asking the server anything. Which matters here: claims are gathered
/// before any collector runs, on the critical path of every run.
///
/// **The host's directory behind the path, by identity rather than by namespace**, measured: the
/// package's unit gives every server a mount namespace of its own and its `dir` is still the host's
/// `/var/lib/redis`, while a redis in a container reports `/data`, a path in its own root. The rule
/// elasticsearch reached first, shared: the same directory on both sides, or the volume or bind
/// mount the mount tables say it is. Nothing where neither side can be read.
///
/// **A bind of part of the directory is that part**, found by review: the host binding only
/// `appendonlydir` at a second path shows the live append-only tree there, which is sealed at its
/// own root rather than read as a data directory holding one.
pub fn server_files_of(proc: &Path, process_id: u32) -> Vec<PathBuf> {
    let Ok(directory) = fs::read_link(proc.join(process_id.to_string()).join("cwd")) else {
        return Vec::new();
    };

    let mut files: Vec<PathBuf> = host_directory_views_of(proc, process_id, &directory)
        .into_iter()
        .flat_map(|view| {
            SERVER_FILES
                .iter()
                .filter_map(|name| shown_at(&view.path, &view.part, Path::new(name)))
                .collect::<Vec<PathBuf>>()
        })
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Where a host path showing `part` of the directory shows `file`: under it where the whole
/// directory or a directory holding the file is shown, the host path itself where it shows the
/// file or something inside it, and nowhere otherwise.
fn shown_at(host: &Path, part: &Path, file: &Path) -> Option<PathBuf> {
    if part.starts_with(file) {
        return Some(host.to_path_buf());
    }
    file.strip_prefix(part).ok().map(|rest| host.join(rest))
}
