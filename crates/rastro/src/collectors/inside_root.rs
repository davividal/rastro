//! Reading a file inside a root, bounded and a regular file only.
//!
//! Shared, because `elasticsearch` reads a node's files inside the node's root, and the host
//! directory lookup that seals a service's data, redis's among them, resolves a path there too.
//! Read as root, a FIFO there blocked the run and `/dev/zero` grew without end, found by the
//! elasticsearch security review, and opening some devices acts on the host, so nothing but a
//! regular file is ever opened for reading.
//!
//! **Measured in the podman VM:** an absolute symlink met under `/proc/<pid>/root` resolves
//! against the reader's root, so a container whose `elasticsearch.yml` links to
//! `/srv/config/elasticsearch.yml` read as not found, or read the host's file at that path.
//! `RESOLVE_IN_ROOT` makes the kernel treat the root as `/` for the whole walk, `..` at the top
//! included.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

/// The most of a file that is read. The largest in either matrix is under 5 KiB.
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

/// The most entries a directory inside a process's root is listed to: an install's `lib/` holds a
/// few hundred, and the directory is its owner's to fill, found by the sweep.
const MOST_LISTED: usize = 10_000;

/// The names in a directory inside a process's root, resolved there as [`read_inside`] resolves a
/// file, and refused past [`MOST_LISTED`].
#[cfg(target_os = "linux")]
pub fn names_inside(root: &Path, relative: &Path) -> std::io::Result<Vec<String>> {
    names_in_pinned_root(&pinned_root(root)?, relative)
}

/// The same on a workstation build, which reads no real service: rastro ships for Linux alone.
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

#[cfg(target_os = "linux")]
fn names_in_pinned_root(
    root: &rustix::fd::OwnedFd,
    relative: &Path,
) -> std::io::Result<Vec<String>> {
    let directory = open_under(root, relative, Opening::List)?;
    // `.` and `..` too, which `read_dir` and so the workstation build leave out.
    bounded(
        rustix::fs::Dir::read_from(&directory)?
            .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
            .filter(|name| !matches!(name.as_deref(), Ok("." | ".."))),
    )
}

/// What a file is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// Its content.
    Read,

    /// Its entries: a directory, opened for reading.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    List,

    /// Only which directory it is, which reads nothing in it.
    Directory,
}

#[cfg(target_os = "linux")]
pub(crate) fn open_inside(root: &Path, relative: &Path, opening: Opening) -> std::io::Result<File> {
    open_under(&pinned_root(root)?, relative, opening)
}

/// The root, pinned without being opened.
#[cfg(target_os = "linux")]
fn pinned_root(root: &Path) -> std::io::Result<rustix::fd::OwnedFd> {
    use rustix::fs::{Mode, OFlags, open};

    Ok(open(
        root,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?)
}

#[cfg(target_os = "linux")]
fn open_under(
    root: &rustix::fd::OwnedFd,
    relative: &Path,
    opening: Opening,
) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, ResolveFlags, openat2};
    use rustix::io::Errno;

    let flags = match opening {
        // Pinned, not opened: an `O_PATH` descriptor reads nothing and opens no device.
        Opening::Read => OFlags::PATH | OFlags::CLOEXEC,
        Opening::List => OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Opening::Directory => OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
    };
    // The root itself is `.`: `openat2` refuses an empty path with `ENOENT`, found on `/`.
    let relative = match relative.as_os_str().is_empty() {
        true => Path::new("."),
        false => relative,
    };
    let file = openat2(
        root,
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
            "this kernel has no openat2, so the path cannot be resolved inside the process's root",
        ),
        other => std::io::Error::from(other),
    })?;

    match opening {
        Opening::Read => reopened_if_regular(&file),
        Opening::List | Opening::Directory => Ok(File::from(file)),
    }
}

/// The filesystems the kernel serves rather than stores, from `linux/magic.h`: proc, sysfs,
/// debugfs, tracefs, securityfs, configfs, bpf, efivarfs, pstore, cgroup, cgroup2 and selinuxfs.
#[cfg(target_os = "linux")]
const KERNEL_FILESYSTEMS: [i64; 12] = [
    0x9fa0,
    0x6265_6572,
    0x6462_6720,
    0x7472_6163,
    0x7363_6673,
    0x6265_6570,
    0xcafe_4a11,
    0xde5e_81e4,
    0x6165_676c,
    0x0027_e0eb,
    0x6367_7270,
    0xf97c_ff8c,
];

/// The file `pinned` names, opened for reading through its own descriptor, where it is a regular
/// file: the inode the type was checked on is the one read, whatever the path now leads to.
///
/// **Nor one the kernel serves**, found by review: `/proc/kmsg` and tracefs's `trace_pipe` are
/// regular files, and reading either takes what it reads from whoever reads next.
#[cfg(target_os = "linux")]
fn reopened_if_regular(pinned: &rustix::fd::OwnedFd) -> std::io::Result<File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let kind = rustix::fs::FileType::from_raw_mode(rustix::fs::fstat(pinned)?.st_mode);
    if kind != rustix::fs::FileType::RegularFile {
        return Err(std::io::Error::other("it is not a regular file"));
    }
    #[allow(clippy::useless_conversion)]
    let filesystem = i64::from(rustix::fs::fstatfs(pinned)?.f_type);
    if KERNEL_FILESYSTEMS.contains(&filesystem) {
        return Err(std::io::Error::other(
            "it is a file the kernel serves rather than stores, which reading can change",
        ));
    }
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC)
        .open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))
}

/// The same on a workstation build, which reads no real service: rastro ships for Linux alone.
#[cfg(not(target_os = "linux"))]
pub(crate) fn open_inside(
    root: &Path,
    relative: &Path,
    _opening: Opening,
) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(root.join(relative))
}
