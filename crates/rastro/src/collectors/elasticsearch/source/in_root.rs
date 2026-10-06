//! Reading a node's files as the node sees them, inside its own root.
//!
//! **Measured in the podman VM:** an absolute symlink met under `/proc/<pid>/root` resolves
//! against the reader's root, so a container whose `elasticsearch.yml` links to
//! `/srv/config/elasticsearch.yml` read as not found, or read the host's file at that path. Not
//! found puts the node on its defaults, the wrong port and the wrong store among them, which a
//! symlink should not decide. `RESOLVE_IN_ROOT`
//! makes the kernel treat the root as `/` for the whole walk, `..` at the top included.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::collectors::elasticsearch::source::mount_table::{host_aliases_of, host_paths_of};
use crate::collectors::inside_root::{Opening, open_inside};

/// Which file a path leads to: the device and the inode, which two paths share only if they are
/// one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

/// The host's own directories that show `path`, canonical, where `path` inside the process's root
/// is a host directory, and none otherwise.
///
/// What decides whether a node's data and log directories may be sealed. Found by review:
/// Elastic's own systemd unit sets `PrivateTmp=true`, so a packaged node has a mount namespace of
/// its own and still keeps its data in the host's `/var/lib/elasticsearch`, while a node in a
/// container names a directory in its image. Comparing namespaces sealed neither; comparing the
/// directory seals the first and not the second. Nothing wherever no side can be read, which
/// makes no claim.
///
/// **A volume or a bind mount is found through the mount tables**, found by the second domain
/// review: there the node's path is not a host path at all, and the kernel says which one it is.
///
/// **Canonical**, found by review: a `path.data` that is a symlink, or spelled with `..`, passed
/// the comparison while the claim named the spelling, and the walk matches paths as text, so it
/// would have walked into the directory behind the link, the live store itself.
///
/// **Every host path that shows it**, found by review: the host can bind the same directory, or a
/// part of it, at a second path, and a walk sealed at one went through the live store at the other.
pub fn host_directories_of(proc: &Path, process_id: u32, path: &Path) -> Vec<PathBuf> {
    let process = proc.join(process_id.to_string());
    let host_table = fs::read(proc.join("self").join("mountinfo")).ok();
    let seen = match same_directory(&process.join("root"), path) {
        true => {
            let mut seen = host_table
                .map(|table| host_aliases_of(&table, path))
                .unwrap_or_default();
            seen.push(path.to_path_buf());
            seen
        }
        false => match (fs::read(process.join("mountinfo")), host_table) {
            (Ok(node_table), Some(host_table)) => host_paths_of(&node_table, &host_table, path),
            _ => Vec::new(),
        },
    };

    let mut directories: Vec<PathBuf> = seen
        .iter()
        .filter(|directory| fs::metadata(directory).is_ok_and(|metadata| metadata.is_dir()))
        .filter_map(|directory| fs::canonicalize(directory).ok())
        .collect();
    directories.sort();
    directories.dedup();
    directories
}

/// Whether `path` inside `root` is the same directory as `path` on rastro's own filesystem.
fn same_directory(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix("/") else {
        return false;
    };
    let host = fs::metadata(path)
        .ok()
        .filter(fs::Metadata::is_dir)
        .map(identity_of);
    let node = open_inside(root, relative, Opening::Directory)
        .and_then(|file| file.metadata())
        .ok()
        .map(identity_of);

    matches!((host, node), (Some(host), Some(node)) if host == node)
}

fn identity_of(metadata: fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}
