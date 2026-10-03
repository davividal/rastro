//! Reading a node's files as the node sees them, inside its own root.
//!
//! **Measured in the podman VM:** an absolute symlink met under `/proc/<pid>/root` resolves
//! against the reader's root, so a container whose `elasticsearch.yml` links to
//! `/srv/config/elasticsearch.yml` read as not found, or read the host's file at that path. Not
//! found puts the node on its defaults, and a default is plaintext, so the gate that keeps rastro
//! from sending plaintext to a TLS listener was the thing a symlink defeated. `RESOLVE_IN_ROOT`
//! makes the kernel treat the root as `/` for the whole walk, `..` at the top included.

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Which file a path leads to: the device and the inode, which two paths share only if they are
/// one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

/// A file read inside a node's root: its text, and when it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFile {
    pub text: String,

    /// The ctime, in seconds since the epoch: when the file's content or metadata last changed.
    /// Not the mtime, which a copy or a tool can set to anything.
    pub changed_at: i64,
}

/// A file's text and change time, every component of `relative` resolved inside `root`.
pub fn read_inside(root: &Path, relative: &Path) -> std::io::Result<ReadFile> {
    let mut file = open_inside(root, relative, Opening::Read)?;
    let changed_at = file.metadata()?.ctime();
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(ReadFile { text, changed_at })
}

/// The host's own directory at `path`, canonical, where `path` inside `root` is that same
/// directory, and nothing otherwise.
///
/// What decides whether a node's data directory may be sealed. Found by review: Elastic's own
/// systemd unit sets `PrivateTmp=true`, so a packaged node has a mount namespace of its own and
/// still keeps its data in the host's `/var/lib/elasticsearch`, while a node in a container names
/// a directory in its image. Comparing namespaces sealed neither; comparing the directory seals
/// the first and not the second. Nothing wherever either side cannot be read, which makes no claim.
///
/// **Canonical**, found by review: a `path.data` that is a symlink, or spelled with `..`, passed
/// the comparison while the claim named the spelling, and the walk matches paths as text, so it
/// would have walked into the directory behind the link, the live store itself.
pub fn host_directory_of(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix("/").ok()?;
    let host = fs::metadata(path)
        .ok()
        .filter(fs::Metadata::is_dir)
        .map(identity_of);
    let node = open_inside(root, relative, Opening::Directory)
        .and_then(|file| file.metadata())
        .ok()
        .map(identity_of);

    match (host, node) {
        (Some(host), Some(node)) if host == node => fs::canonicalize(path).ok(),
        _ => None,
    }
}

fn identity_of(metadata: fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

/// What a file is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opening {
    /// Its content.
    Read,

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
        Opening::Read => OFlags::RDONLY | OFlags::CLOEXEC,
        Opening::Directory => OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
    };
    let file = openat2(&root, relative, flags, Mode::empty(), ResolveFlags::IN_ROOT).map_err(
        |errno| match errno {
            // Before Linux 5.6 there is no safe way to walk another root, and reading past it is
            // the defect this exists to prevent, so the read is refused rather than approximated.
            Errno::NOSYS => std::io::Error::other(
                "this kernel has no openat2, so the path cannot be resolved inside the node's root",
            ),
            other => std::io::Error::from(other),
        },
    )?;

    Ok(File::from(file))
}

/// The same on a workstation build, which reads no real node: rastro ships for Linux alone.
#[cfg(not(target_os = "linux"))]
fn open_inside(root: &Path, relative: &Path, _opening: Opening) -> std::io::Result<File> {
    File::open(root.join(relative))
}
