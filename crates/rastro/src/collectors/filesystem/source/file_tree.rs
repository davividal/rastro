//! The filesystem interface: a tree, read one directory at a time.
//!
//! Everything peculiar to reading a real filesystem lives here: that `read_dir` returns
//! entries in the filesystem's own order, that `stat` follows a symlink and `lstat` does
//! not, and that a path is bytes rather than text.
//!
//! **Symlinks are never followed.** `symlink_metadata` throughout, and the walk descends
//! only into real directories. Following them would report a target's content under two
//! paths, lose the link itself, and loop the moment a directory links to its own ancestor.
//! An enablement symlink under a `*.wants/` directory is the thing this walker exists to
//! catch, so the link is the state.
//!
//! **The walk stops at a mount boundary, and a device number is not enough to find one.** A
//! bind mount shares the device of what it binds: `mount --bind / /mnt/root` leaves both at
//! device 2049, verified on the reference box, so comparing devices alone walked and hashed
//! the whole root filesystem a second time under `/mnt/root`. The caller therefore hands in
//! the mount points, and the walk stops at each of them.
//!
//! The device comparison is kept as a second guard, for a mount the table did not name. Either
//! way the directory is recorded and not descended into: a mount appears in the document as the
//! directory it is, and whether its contents are walked is decided by whoever chose the
//! roots.
//!
//! **One path's failure is that path's failure.** A path that vanished mid-walk is omitted
//! and one that will not be read is recorded with the reason, so a log rotating under the
//! walk or an unreadable fuse mount costs one entry rather than the whole facet. Two failures
//! stay fatal: the root's own stat, because a walk that cannot start is not an empty host, and
//! a path whose *bytes* cannot be read at all. A name that is merely not UTF-8 is reported in
//! a list of its own, because it has no key to be filed under but is still a fact about the
//! box — and one such name used to cost the entire facet.
//!
//! **Still owed, and deliberately not here yet:**
//!
//! - POSIX ACLs, SELinux labels and the rest of the extended attributes. `docs/design.md`
//!   promises them and a capability-only change is invisible without them, but every one of
//!   them needs an `*xattr(2)` call this walk has no seam for yet, and the answer to "which
//!   attributes" is one decision rather than three. See `docs/decisions.md`.

use std::fs::{self, Metadata};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rastro_collector::{AbsolutePath, ByteSize, CollectionError, NonEmptyText};
use sha2::{Digest as _, Sha256};

use crate::collectors::filesystem::model::{
    FileEntry, FilesystemInventory, UnreadablePath, UnspellablePath, WalkPolicy, is_absence,
};
use crate::collectors::filesystem::value_objects::{
    ContentPolicy, DeviceNumber, Digest, DigestAlgorithm, FileKind, FileMode, NanosecondsSinceEpoch,
};
use crate::progress::WalkProgress;

/// A tree to walk, rooted where the caller says.
///
/// The root is a parameter rather than `/` so the walk can be exercised against a scratch
/// directory, and so a future scope can hand it one mount at a time.
pub struct FileTree {
    root: PathBuf,
    boundaries: Vec<PathBuf>,
    omitted: Vec<PathBuf>,
    progress: Option<Arc<dyn WalkProgress>>,
}

impl FileTree {
    pub fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            boundaries: Vec::new(),
            omitted: Vec::new(),
            progress: None,
        }
    }

    /// The same walk, reporting everything but these files.
    ///
    /// Two paths reach here, and both are rastro's own footprint rather than the host's. The
    /// executable, when a caller staged a temporary copy: `rastro-ssh` puts it in `/var/tmp`,
    /// which is walked on purpose, so without this every remote run reports one added and one
    /// removed file under a fresh name. And the document this run is about to write, because
    /// the second run of `-o before.json` would otherwise find the first one sitting there and
    /// two runs of an unchanged host would stop being byte-identical.
    ///
    /// Whole paths, compared whole. Not a name pattern, because a file an operator called
    /// `rastro` is theirs and belongs in the document.
    pub fn omitting(mut self, paths: &[PathBuf]) -> Self {
        self.omitted = paths.to_vec();

        self
    }

    /// The same walk, counting what it does for whoever is watching.
    ///
    /// `Arc` rather than a borrow, because collectors live in a `Vec<Box<dyn Collector>>` and
    /// a lifetime here would infect that whole registry — and shared rather than owned,
    /// because the sink watching this walk is watching every other collector too, from
    /// whichever thread each is running on.
    pub fn reporting_to(mut self, progress: Arc<dyn WalkProgress>) -> Self {
        self.progress = Some(progress);

        self
    }

    /// The same walk, stopping at each of these paths.
    ///
    /// The root itself is never a boundary to its own walk, whatever the caller passes: it is
    /// a mount point in every real call, and stopping there would walk nothing.
    pub fn stopping_at(mut self, boundaries: &[&Path]) -> Self {
        self.boundaries = boundaries
            .iter()
            .map(|boundary| boundary.to_path_buf())
            .filter(|boundary| boundary != &self.root)
            .collect();

        self
    }

    /// Every entry under the root, the root included, ordered by path.
    ///
    /// A walk that cannot start is a failure rather than an empty tree: an empty inventory
    /// would read as a host with no files on it.
    pub fn walk(&self, policy: &WalkPolicy) -> Result<FilesystemInventory, CollectionError> {
        let mut entries = Vec::new();
        let mut unreadable = Vec::new();
        let mut unspellable = Vec::new();
        let mut pending = vec![self.root.clone()];
        let device = fs::symlink_metadata(&self.root)
            .map_err(|error| {
                CollectionError::new(format!(
                    "{} could not be read, so the walk cannot start: {error}",
                    self.root.display()
                ))
            })?
            .dev();

        while let Some(path) = pending.pop() {
            if self.omitted.contains(&path) {
                continue;
            }

            // A path that will not decode has no key to be filed under, so it is reported in
            // a list of its own rather than costing the facet. Recorded and not descended
            // into: every name beneath it is unspellable too, and the directory is the fact.
            let Some(recorded) = absolute(&path) else {
                unspellable.push(UnspellablePath::of(
                    path.file_name().unwrap_or(path.as_os_str()).as_bytes(),
                    path.parent().and_then(Path::to_str).map(str::to_owned),
                ));
                continue;
            };

            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    note(
                        &mut unreadable,
                        &recorded,
                        failure(&path, "could not be read", &error),
                    );
                    continue;
                }
            };

            let entry = match self.entry_of(&path, &recorded, &metadata, policy) {
                Ok(entry) => entry,
                Err(refusal) => {
                    note(&mut unreadable, &recorded, refusal);
                    continue;
                }
            };

            if entry.kind == FileKind::Directory
                && entry.reading.is_descended()
                && metadata.dev() == device
                && !self.boundaries.contains(&path)
            {
                match self.children_of(&path) {
                    Ok(children) => pending.extend(children),
                    Err(refusal) => {
                        // The directory loses its own entry too, on the same rule the whole
                        // change rests on: a path is its attributes or the reason it has
                        // none. Recording mode and owner beside a failed listing would read
                        // as a complete description of a directory nobody could enumerate.
                        note(&mut unreadable, &recorded, refusal);
                        continue;
                    }
                }
            }

            if let Some(progress) = &self.progress {
                progress.entry_walked();
            }

            entries.push(entry);
        }

        FilesystemInventory::new(entries, unreadable, unspellable)
    }

    /// The paths directly inside a directory.
    ///
    /// Collected rather than streamed so the read is over before the walk descends, which
    /// keeps one open directory handle per level instead of one per entry.
    fn children_of(&self, directory: &Path) -> Result<Vec<PathBuf>, Refusal> {
        let reader = fs::read_dir(directory)
            .map_err(|error| failure(directory, "could not be listed", &error))?;

        reader
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| failure(directory, "could not be listed fully", &error))
            })
            .collect()
    }

    fn entry_of(
        &self,
        path: &Path,
        recorded: &AbsolutePath,
        metadata: &Metadata,
        policy: &WalkPolicy,
    ) -> Result<FileEntry, Refusal> {
        let kind = kind_of(metadata);

        let reading = policy.policy_for(recorded).clone();

        Ok(FileEntry {
            kind,
            mode: FileMode::of(metadata.mode()),
            owner: i64::from(metadata.uid()),
            group: i64::from(metadata.gid()),
            size: size_of(kind, metadata)?,
            modified: stamp(
                metadata.mtime(),
                metadata.mtime_nsec(),
                "a modification time",
                path,
            )?,
            changed: stamp(
                metadata.ctime(),
                metadata.ctime_nsec(),
                "a status change time",
                path,
            )?,
            inode: count(metadata.ino(), "an inode number", path)?,
            link_count: count(metadata.nlink(), "a link count", path)?,
            link_target: link_target_of(kind, path)?,
            device: device_of(kind, metadata),
            digest: self.digest_of(kind, path, &reading)?,
            reading,
            path: recorded.clone(),
        })
    }

    /// The digest of a file's content, when there is content and the policy asks for it.
    fn digest_of(
        &self,
        kind: FileKind,
        path: &Path,
        policy: &ContentPolicy,
    ) -> Result<Option<Digest>, Refusal> {
        let ContentPolicy::Hashed(algorithm) = policy else {
            return Ok(None);
        };

        if !kind.has_content() {
            return Ok(None);
        }

        match algorithm {
            DigestAlgorithm::Sha256 => Ok(Some(sha256_of(path)?)),
        }
    }
}

/// Why one path is not in the document as itself.
///
/// The two answers have opposite consequences for the byte-identical guarantee, which is why
/// this is a type rather than one error with a message: see
/// [`is_absence`](crate::collectors::filesystem::is_absence).
enum Refusal {
    /// It went away between the walk listing its parent and reaching it.
    Gone,
    /// It is there and would not describe itself.
    Unreadable(String),
}

/// Records a refusal, or nothing at all when the path simply went away.
fn note(unreadable: &mut Vec<UnreadablePath>, path: &AbsolutePath, refusal: Refusal) {
    let Refusal::Unreadable(reason) = refusal else {
        return;
    };

    unreadable.push(UnreadablePath {
        path: path.clone(),
        reason: NonEmptyText::new(reason, "a refusal")
            .expect("a refusal carries at least a path and a reason"),
    });
}

/// Hashes a file without holding it in memory.
///
/// `io::copy` streams it through the hasher, so a multi-gigabyte file costs a buffer
/// rather than its own size. rastro runs as root on production and must not be the reason
/// a box runs out of memory.
fn sha256_of(path: &Path) -> Result<Digest, Refusal> {
    let mut file = open_without_following(path)?;
    let mut hasher = Sha256::new();

    io::copy(&mut file, &mut hasher)
        .map_err(|error| failure(path, "could not be read to the end", &error))?;

    Ok(Digest::of(DigestAlgorithm::Sha256, &hasher.finalize()))
}

/// Opens a file for hashing in a way the path cannot be swapped out from under.
///
/// The `symlink_metadata` that said "regular file" and this open are two calls, and on a
/// live box a package upgrade can land between them. Opening by pathname would then follow
/// a replacement symlink out of the walked tree, or block forever on a replacement fifo —
/// as root, on production, with the never-follow promise this module makes broken silently.
///
/// `O_NOFOLLOW` refuses the symlink, `O_NONBLOCK` refuses to wait on anything that would
/// have made the open itself hang, and the type is checked again on the descriptor rather
/// than on the path, because only the descriptor is the thing being read.
fn open_without_following(path: &Path) -> Result<fs::File, Refusal> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| failure(path, "could not be opened", &error))?;

    let opened = file
        .metadata()
        .map_err(|error| failure(path, "could not be described once open", &error))?;

    if !opened.file_type().is_file() {
        return Err(Refusal::Unreadable(format!(
            "{} stopped being a regular file while it was being read, so no digest of it is honest",
            path.display()
        )));
    }

    Ok(file)
}

/// Which of the seven kinds this entry is.
///
/// Ordered with the two common cases first, and symlink before the rest because
/// `is_file` and `is_dir` are already false for one.
fn kind_of(metadata: &Metadata) -> FileKind {
    let kind = metadata.file_type();

    if kind.is_file() {
        FileKind::Regular
    } else if kind.is_dir() {
        FileKind::Directory
    } else if kind.is_symlink() {
        FileKind::Symlink
    } else if kind.is_fifo() {
        FileKind::Fifo
    } else if kind.is_socket() {
        FileKind::Socket
    } else if kind.is_block_device() {
        FileKind::BlockDevice
    } else {
        FileKind::CharacterDevice
    }
}

fn size_of(kind: FileKind, metadata: &Metadata) -> Result<Option<ByteSize>, Refusal> {
    if !kind.has_content() {
        return Ok(None);
    }

    ByteSize::new(metadata.len(), "file size")
        .map(Some)
        .map_err(|error| Refusal::Unreadable(error.to_string()))
}

fn link_target_of(kind: FileKind, path: &Path) -> Result<Option<String>, Refusal> {
    if kind != FileKind::Symlink {
        return Ok(None);
    }

    let target =
        fs::read_link(path).map_err(|error| failure(path, "could not be resolved", &error))?;

    // A target that will not spell is a refusal rather than the end of the walk, unlike a
    // *path* that will not: the key is the path, so this one has somewhere to be recorded.
    text_of(&target)
        .map(Some)
        .map_err(|error| Refusal::Unreadable(error.to_string()))
}

/// A path as text, or a refusal.
///
/// Linux paths are bytes, and rastro's document holds text. Substituting `U+FFFD` for what
/// will not decode is the operation `canonical_tool` refuses for a tool's output, and the
/// reason is the same here: it would put a path into the fingerprint that is not on the
/// box, and one nobody could act on.
fn text_of(path: &Path) -> Result<String, CollectionError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        CollectionError::new(format!(
            "{} is not valid UTF-8, so it cannot be recorded as the path it is",
            path.display()
        ))
    })
}

/// The numbers a device node addresses, and nothing for anything else.
///
/// `st_rdev` is zero on a regular file and undefined on some filesystems, so reading it
/// unconditionally would record a number that means nothing for most of the tree.
fn device_of(kind: FileKind, metadata: &Metadata) -> Option<DeviceNumber> {
    if !kind.is_device() {
        return None;
    }

    Some(DeviceNumber::of(metadata.rdev()))
}

fn stamp(
    seconds: i64,
    nanoseconds: i64,
    kind: &str,
    path: &Path,
) -> Result<NanosecondsSinceEpoch, Refusal> {
    NanosecondsSinceEpoch::of(seconds, nanoseconds).ok_or_else(|| {
        Refusal::Unreadable(format!(
            "{kind} of {seconds}s and {nanoseconds}ns on {} is too far from the epoch to record",
            path.display()
        ))
    })
}

/// The path as the document keys it, or nothing when it will not decode.
///
/// An `Option` rather than a failure, because the caller reports the undecodable name in a
/// list of its own and carries on. `AbsolutePath::new` cannot fail for a walked path: the
/// walk starts at an absolute root and only ever appends to it.
fn absolute(path: &Path) -> Option<AbsolutePath> {
    AbsolutePath::new(path.to_str()?, "walked path").ok()
}

fn count(value: u64, kind: &str, path: &Path) -> Result<i64, Refusal> {
    i64::try_from(value).map_err(|_| {
        Refusal::Unreadable(format!(
            "{kind} of {value} on {} is too large to record as an integer",
            path.display()
        ))
    })
}

/// One io failure at one path, classified into the two answers the walk has for it.
fn failure(path: &Path, what_happened: &str, error: &io::Error) -> Refusal {
    if is_absence(error.kind()) {
        return Refusal::Gone;
    }

    Refusal::Unreadable(format!("{} {what_happened}: {error}", path.display()))
}
