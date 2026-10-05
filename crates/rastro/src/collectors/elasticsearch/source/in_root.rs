//! Reading a node's files as the node sees them, inside its own root.
//!
//! **Measured in the podman VM:** an absolute symlink met under `/proc/<pid>/root` resolves
//! against the reader's root, so a container whose `elasticsearch.yml` links to
//! `/srv/config/elasticsearch.yml` read as not found, or read the host's file at that path. Not
//! found puts the node on its defaults, the wrong port and the wrong store among them, which a
//! symlink should not decide. `RESOLVE_IN_ROOT`
//! makes the kernel treat the root as `/` for the whole walk, `..` at the top included.

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::collectors::elasticsearch::source::mount_table::{host_aliases_of, host_paths_of};

/// Which file a path leads to: the device and the inode, which two paths share only if they are
/// one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

/// The most of a node's file that is read. The largest file in the matrix is under 5 KiB.
const MOST_READ: u64 = 1024 * 1024;

/// A file's text, every component of `relative` resolved inside `root`.
///
/// **Bounded, and a regular file only**, found by the security review: the root is the
/// process's, so its owner chooses what is at the path, and read as root a FIFO blocked the run
/// and `/dev/zero` grew without end. **Nothing else is opened for reading**, found by the next
/// one: opening some devices acts on the host, a watchdog being armed by it. So the path is
/// pinned without being opened, its type checked on the pin, and only a regular file reopened.
pub fn read_inside(root: &Path, relative: &Path) -> std::io::Result<String> {
    let file = open_inside(root, relative, Opening::Read)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("it is not a regular file"));
    }
    let mut text = String::new();
    file.take(MOST_READ + 1).read_to_string(&mut text)?;
    match u64::try_from(text.len()).is_ok_and(|length| length <= MOST_READ) {
        true => Ok(text),
        false => Err(std::io::Error::other(format!(
            "it is larger than {MOST_READ} bytes"
        ))),
    }
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
    let host_table = fs::read_to_string(proc.join("self").join("mountinfo")).ok();
    let seen = match same_directory(&process.join("root"), path) {
        true => {
            let mut seen = host_table
                .map(|table| host_aliases_of(&table, path))
                .unwrap_or_default();
            seen.push(path.to_path_buf());
            seen
        }
        false => match (fs::read_to_string(process.join("mountinfo")), host_table) {
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

/// The most entries a directory inside a node's root is listed to: an install's `lib/` holds a few
/// hundred, and the directory is its owner's to fill, found by the sweep.
const MOST_LISTED: usize = 10_000;

/// The names in a directory inside a node's root, resolved there as [`read_inside`] resolves a file,
/// and refused past [`MOST_LISTED`].
#[cfg(target_os = "linux")]
pub fn names_inside(root: &Path, relative: &Path) -> std::io::Result<Vec<String>> {
    let directory = open_inside(root, relative, Opening::List)?;
    bounded(
        rustix::fs::Dir::read_from(&directory)?
            .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned())),
    )
}

/// The same on a workstation build, which reads no real node: rastro ships for Linux alone.
#[cfg(not(target_os = "linux"))]
pub fn names_inside(root: &Path, relative: &Path) -> std::io::Result<Vec<String>> {
    bounded(
        fs::read_dir(root.join(relative))?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned())),
    )
}

fn bounded(names: impl Iterator<Item = std::io::Result<String>>) -> std::io::Result<Vec<String>> {
    let names: Vec<String> = names
        .take(MOST_LISTED + 1)
        .collect::<std::io::Result<_>>()?;
    match names.len() > MOST_LISTED {
        true => Err(std::io::Error::other(format!(
            "it holds more than {MOST_LISTED} entries"
        ))),
        false => Ok(names),
    }
}

/// What a file is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opening {
    /// Its content.
    Read,

    /// Its entries: a directory, opened for reading.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    List,

    /// Only which directory it is, which reads nothing in it.
    Directory,
}

#[cfg(target_os = "linux")]
fn open_inside(root: &Path, relative: &Path, opening: Opening) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, ResolveFlags, open, openat2};
    use rustix::io::Errno;

    let root = open(
        root,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let flags = match opening {
        // Pinned, not opened: an `O_PATH` descriptor reads nothing and opens no device.
        Opening::Read => OFlags::PATH | OFlags::CLOEXEC,
        Opening::List => OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Opening::Directory => OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
    };
    let file = openat2(
        &root,
        relative,
        flags,
        Mode::empty(),
        // Explicit, since the man page says `IN_ROOT` disabling magic links may change.
        ResolveFlags::IN_ROOT | ResolveFlags::NO_MAGICLINKS,
    )
    .map_err(|errno| match errno {
        // Before Linux 5.6 there is no safe way to walk another root, and reading past it is
        // the defect this exists to prevent, so the read is refused rather than approximated.
        Errno::NOSYS => std::io::Error::other(
            "this kernel has no openat2, so the path cannot be resolved inside the node's root",
        ),
        other => std::io::Error::from(other),
    })?;

    match opening {
        Opening::Read => reopened_if_regular(&file),
        Opening::List | Opening::Directory => Ok(File::from(file)),
    }
}

/// The file `pinned` names, opened for reading through its own descriptor, where it is a regular
/// file: the inode the type was checked on is the one read, whatever the path now leads to.
#[cfg(target_os = "linux")]
fn reopened_if_regular(pinned: &rustix::fd::OwnedFd) -> std::io::Result<File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let kind = rustix::fs::FileType::from_raw_mode(rustix::fs::fstat(pinned)?.st_mode);
    if kind != rustix::fs::FileType::RegularFile {
        return Err(std::io::Error::other("it is not a regular file"));
    }
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC)
        .open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))
}

/// The same on a workstation build, which reads no real node: rastro ships for Linux alone.
#[cfg(not(target_os = "linux"))]
fn open_inside(root: &Path, relative: &Path, _opening: Opening) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(root.join(relative))
}
