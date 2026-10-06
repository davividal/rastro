//! The directory a server writes into, from its working directory.

use std::fs;
use std::path::{Path, PathBuf};

use super::mount_namespace::shares_our_mounts;

/// Where a server keeps its dump and its append-only files, or nothing where it cannot be told.
///
/// **The working directory is the setting.** redis applies `dir` with `chdir`, at start and on
/// every `CONFIG SET dir`, and `CONFIG GET dir` answers with `getcwd`, so `/proc/<pid>/cwd` is
/// the directory without asking the server anything. Which matters here: claims are gathered
/// before any collector runs, on the critical path of every run.
///
/// **The root is never it.** `dir ./` in a server started from `/` leaves the working directory at
/// the root, and sealing that would seal the whole walk to hide one dump file.
///
/// **Only for a server in rastro's own mount namespace.** A redis in a container reports its
/// working directory in its own namespace, `/data` measured, and sealed as it stands that would
/// hide the host's `/data`. Where the two cannot be compared nothing is claimed either: the walk's
/// default is the safe direction to be wrong in.
pub fn data_directory_of(proc: &Path, process_id: u32) -> Option<PathBuf> {
    if !shares_our_mounts(proc, process_id) {
        return None;
    }

    let directory = fs::read_link(proc.join(process_id.to_string()).join("cwd")).ok()?;

    (directory.parent().is_some()).then_some(directory)
}
