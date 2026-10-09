//! The directory a server writes into, from its working directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::collectors::host_directories::host_directories_of;

/// The host's directories that hold a server's dump and append-only files, or none where that
/// cannot be told.
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
pub fn data_directories_of(proc: &Path, process_id: u32) -> Vec<PathBuf> {
    let Ok(directory) = fs::read_link(proc.join(process_id.to_string()).join("cwd")) else {
        return Vec::new();
    };

    host_directories_of(proc, process_id, &directory)
}
