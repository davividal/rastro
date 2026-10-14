//! Reading a file inside a root, bounded and a regular file only.
//!
//! Shared, because two collectors read files a service's own account controls: `elasticsearch`
//! reads a node's files inside the node's root, and `redis` reads the configuration a server's unit
//! names. Read as root, a FIFO there blocked the run and `/dev/zero` grew without end, found by
//! the elasticsearch security review, and opening some devices acts on the host, so nothing but a
//! regular file is ever opened for reading.
//!
//! **Measured in the podman VM:** an absolute symlink met under `/proc/<pid>/root` resolves
//! against the reader's root, so a container whose `elasticsearch.yml` links to
//! `/srv/config/elasticsearch.yml` read as not found, or read the host's file at that path.
//! `RESOLVE_IN_ROOT` makes the kernel treat the root as `/` for the whole walk, `..` at the top
//! included.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

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
    String::from_utf8(read_bytes_inside(root, relative)?).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        )
    })
}

/// The same file's bytes, for a reader that decodes it a part at a time.
pub fn read_bytes_inside(root: &Path, relative: &Path) -> std::io::Result<Vec<u8>> {
    bounded_bytes(open_inside(root, relative, Opening::Read)?)
}

fn bounded_bytes(file: File) -> std::io::Result<Vec<u8>> {
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("it is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MOST_READ + 1).read_to_end(&mut bytes)?;
    match u64::try_from(bytes.len()).is_ok_and(|length| length <= MOST_READ) {
        true => Ok(bytes),
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

/// The filesystem identity a read is made with: a process's own, as `/proc/<pid>/status` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingAccount {
    user: u32,
    group: u32,
    groups: Vec<u32>,
}

impl ReadingAccount {
    pub fn new(user: u32, group: u32, groups: Vec<u32>) -> Self {
        Self {
            user,
            group,
            groups,
        }
    }

    /// The filesystem user, group and supplementary groups the process opens files with.
    pub fn of_process(proc: &Path, process_id: u32) -> std::io::Result<Self> {
        let status = fs::read_to_string(proc.join(process_id.to_string()).join("status"))?;
        let line = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .ok_or_else(|| std::io::Error::other(format!("its status has no {name:?} line")))
        };
        // `Uid:` and `Gid:` list the real, effective, saved and filesystem ids, in that order.
        let filesystem_id = |name: &str| -> std::io::Result<u32> {
            line(name)?
                .split_whitespace()
                .nth(3)
                .and_then(|id| id.parse().ok())
                .ok_or_else(|| {
                    std::io::Error::other(format!("its status {name:?} line is malformed"))
                })
        };
        let groups = line("Groups:")?
            .split_whitespace()
            .map(|group| {
                group
                    .parse()
                    .map_err(|_| std::io::Error::other("its status \"Groups:\" line is malformed"))
            })
            .collect::<std::io::Result<Vec<u32>>>()?;

        Ok(Self::new(
            filesystem_id("Uid:")?,
            filesystem_id("Gid:")?,
            groups,
        ))
    }

    pub fn user(&self) -> u32 {
        self.user
    }

    pub fn group(&self) -> u32 {
        self.group
    }
}

/// A process's root, read with the process's own permission.
///
/// **Its permission, not rastro's**, found by review: a service's account may edit its own
/// configuration, Debian's `/etc/redis` among it, and an include of a file only root may read
/// would have root-run rastro read it on that account's behalf. The kernel makes every check, of
/// each directory on the way, the file's mode and its ACL, because the read is made in a thread
/// holding the process's effective ids and groups, which ends with the read. Only the effective
/// ids change: the real and saved ones stay root's, so the account cannot signal that thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRoot {
    path: PathBuf,
    account: ReadingAccount,
}

impl ProcessRoot {
    pub fn new(path: impl Into<PathBuf>, account: ReadingAccount) -> Self {
        Self {
            path: path.into(),
            account,
        }
    }

    /// The root of a process on the box, read as that process.
    pub fn of_process(proc: &Path, process_id: u32) -> std::io::Result<Self> {
        Ok(Self::new(
            proc.join(process_id.to_string()).join("root"),
            ReadingAccount::of_process(proc, process_id)?,
        ))
    }

    /// A file's bytes, as [`read_bytes_inside`] reads them, where the process may read it.
    pub fn read_bytes(&self, relative: &Path) -> std::io::Result<Vec<u8>> {
        self.as_account(|root| read_pinned_root(root, relative))
    }

    /// A directory's names, as [`names_inside`] lists them, where the process may list it.
    pub fn names(&self, relative: &Path) -> std::io::Result<Vec<String>> {
        self.as_account(|root| names_in_pinned_root(root, relative))
    }

    /// `work` done on the root, pinned as rastro, with the process's permission.
    #[cfg(target_os = "linux")]
    fn as_account<T: Send>(
        &self,
        work: impl FnOnce(&rustix::fd::OwnedFd) -> std::io::Result<T> + Send,
    ) -> std::io::Result<T> {
        let root = pinned_root(&self.path)?;
        let ours = rustix::process::geteuid().as_raw();
        match (ours, self.account.user) {
            (_, theirs) if theirs == ours => work(&root),
            (0, _) => std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        taken_on(&self.account)?;
                        work(&root)
                    })
                    .join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("the read panicked")))
            }),
            _ => Err(std::io::Error::other(
                "rastro runs unprivileged, so it cannot read the file with the permission of the \
                 account that owns the process",
            )),
        }
    }

    /// The same on a workstation build, which reads no real server: rastro ships for Linux alone.
    #[cfg(not(target_os = "linux"))]
    fn as_account<T>(&self, work: impl FnOnce(&Path) -> std::io::Result<T>) -> std::io::Result<T> {
        work(&self.path)
    }
}

/// This thread's effective ids and groups made the account's, for the rest of its life.
///
/// Groups first, while the thread still has the capability to set them, and the user last, which
/// gives up the rest.
#[cfg(target_os = "linux")]
fn taken_on(account: &ReadingAccount) -> std::io::Result<()> {
    use rustix::process::{Gid, Uid};
    use rustix::thread::{set_thread_groups, set_thread_res_gid, set_thread_res_uid};

    let groups: Vec<Gid> = account
        .groups
        .iter()
        .map(|group| Gid::from_raw(*group))
        .collect();
    set_thread_groups(&groups)?;
    set_thread_res_gid(None, Gid::from_raw(account.group), None)?;
    set_thread_res_uid(None, Uid::from_raw(account.user), None)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_pinned_root(root: &rustix::fd::OwnedFd, relative: &Path) -> std::io::Result<Vec<u8>> {
    bounded_bytes(open_under(root, relative, Opening::Read)?)
}

#[cfg(not(target_os = "linux"))]
fn read_pinned_root(root: &Path, relative: &Path) -> std::io::Result<Vec<u8>> {
    read_bytes_inside(root, relative)
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

#[cfg(not(target_os = "linux"))]
fn names_in_pinned_root(root: &Path, relative: &Path) -> std::io::Result<Vec<String>> {
    names_inside(root, relative)
}

/// `connect` given a path that reaches the unix socket at `relative` inside `root`, resolved there
/// as [`read_inside`] resolves a file, and only where it is a socket.
///
/// **Through the pinned socket's own descriptor**, `/proc/self/fd/<n>`, which `connect` follows to
/// the inode: the path inside the root is resolved by `openat2` rather than by `connect`, which
/// would resolve an absolute symlink against rastro's root, and a long path inside a deep root
/// stays under the hundred or so bytes a socket address holds.
#[cfg(target_os = "linux")]
pub fn connect_inside<T>(
    root: &Path,
    relative: &Path,
    connect: impl FnOnce(&Path) -> std::io::Result<T>,
) -> std::io::Result<T> {
    use std::os::fd::AsRawFd;

    let socket = open_under(&pinned_root(root)?, relative, Opening::Socket)?;
    connect(Path::new(&format!("/proc/self/fd/{}", socket.as_raw_fd())))
}

/// The same on a workstation build, which reads no real server: rastro ships for Linux alone.
#[cfg(not(target_os = "linux"))]
pub fn connect_inside<T>(
    root: &Path,
    relative: &Path,
    connect: impl FnOnce(&Path) -> std::io::Result<T>,
) -> std::io::Result<T> {
    connect(&fs::canonicalize(root)?.join(relative))
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

    /// Only which socket it is, for a connection through its descriptor.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Socket,
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
        Opening::Socket => OFlags::PATH | OFlags::CLOEXEC,
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
        Opening::Socket => {
            let kind = rustix::fs::FileType::from_raw_mode(rustix::fs::fstat(&file)?.st_mode);
            match kind == rustix::fs::FileType::Socket {
                true => Ok(File::from(file)),
                false => Err(std::io::Error::other("it is not a socket")),
            }
        }
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
