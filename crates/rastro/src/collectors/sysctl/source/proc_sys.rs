//! The `/proc/sys` interface.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::proc_sys_entry;
use crate::collectors::sysctl::model::SysctlParameters;
use crate::collectors::sysctl::value_objects::{SysctlKey, SysctlValue};

/// Where the kernel publishes its runtime parameters.
///
/// Effective state over declared state: `/etc/sysctl.conf` and the `sysctl.d`
/// drop-ins say what should have been applied at boot, this says what the kernel is
/// running with. The gap between them is exactly what a fingerprint is for, since a
/// parameter set by hand with `sysctl -w` appears in no file at all.
const PROC_SYS: &str = "/proc/sys";

/// Where `/proc` itself is, which is how a kernel with no sysctl support is told
/// apart from a host whose `/proc` was never mounted.
const PROC: &str = "/proc";

/// The entry that proves a procfs is actually mounted rather than merely present.
///
/// The same reasoning as the module collector's, and the same entry: `/proc` is a
/// directory in Debian's base layout whether or not anything is mounted on it, and
/// `self` is provided by procfs and by nothing else.
const PROC_SELF: &str = "self";

/// Why rastro shells out to nothing here.
///
/// `sysctl -a` exists and would have been the canonical tool, and it was rejected on
/// the format rather than on principle. It prints `key = value`, which is ambiguous
/// the moment a value contains ` = `, and it flattens the multi-line values under
/// `fs.binfmt_misc` onto lines that cannot be told from separate parameters. The
/// tree itself is unambiguous: one file is one parameter, its path is the name and
/// its contents are the value. The rule in `design.md` is to prefer the source that
/// is unambiguous, not to shell out on reflex.
///
/// The walk is also what makes the write-only triggers visible as such, because it
/// sees the mode. `sysctl -a` silently omits them and offers no way to tell that
/// from a parameter that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcSys {
    root: PathBuf,
    procfs: PathBuf,
}

impl ProcSys {
    pub fn new() -> Self {
        Self {
            root: PathBuf::from(PROC_SYS),
            procfs: PathBuf::from(PROC),
        }
    }

    pub fn at(root: impl Into<PathBuf>, procfs: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            procfs: procfs.into(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the interface's filesystem is expected to be mounted.
    pub fn filesystem(&self) -> &Path {
        &self.procfs
    }

    pub fn exists(&self) -> bool {
        self.root.is_dir()
    }

    /// Whether the interface's filesystem is mounted at all.
    pub fn filesystem_is_mounted(&self) -> bool {
        self.procfs.join(PROC_SELF).exists()
    }

    /// Walks the tree and translates every parameter in it.
    ///
    /// **Depth-first over a stack rather than recursion**, because the depth of this
    /// tree is not rastro's to bound: `net.ipv4.conf.<interface>.<parameter>` is
    /// already five levels, and a container runtime can nest network namespaces
    /// further. A recursive walk would put that depth on the thread's stack.
    ///
    /// **Directories are entered, files are read, and nothing else is touched.** The
    /// kernel publishes only those two kinds under this root, so anything else
    /// arriving here is not a parameter. `symlink_metadata` is what asks, so a
    /// symlink is never followed: this tree contains none today, and following one
    /// out of it would let a mount elsewhere on the box masquerade as kernel state.
    pub fn read(&self) -> Result<SysctlParameters, CollectionError> {
        let mut parameters = Vec::new();
        let mut pending = vec![Vec::<String>::new()];

        while let Some(segments) = pending.pop() {
            for entry in self.children(&segments)? {
                match entry {
                    Child::Directory(segments) => pending.push(segments),
                    Child::Parameter(parameter) => parameters.push(parameter),
                }
            }
        }

        Ok(SysctlParameters::new(parameters))
    }

    /// Everything one directory of the tree holds, already translated.
    ///
    /// The directory's own entries are sorted before being handed back, which the
    /// model does not need and a reader of a failure does. Without it the parameter
    /// a run failed on would vary between two runs over the same tree, since
    /// `read_dir` yields in the filesystem's order.
    fn children(&self, segments: &[String]) -> Result<Vec<Child>, CollectionError> {
        let directory = self.path_of(segments);
        let mut names = Vec::new();

        for entry in fs::read_dir(&directory).map_err(|error| {
            CollectionError::new(format!("could not list {}: {error}", directory.display()))
        })? {
            let entry = entry.map_err(|error| {
                CollectionError::new(format!(
                    "could not list an entry of {}: {error}",
                    directory.display()
                ))
            })?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();

        names
            .into_iter()
            .map(|name| self.child(segments, name))
            .filter_map(Result::transpose)
            .collect()
    }

    /// What one name inside a directory is, or nothing if it is neither kind.
    fn child(&self, segments: &[String], name: String) -> Result<Option<Child>, CollectionError> {
        let mut segments = segments.to_vec();
        segments.push(name);

        let path = self.path_of(&segments);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            CollectionError::new(format!("could not stat {}: {error}", path.display()))
        })?;

        if metadata.is_dir() {
            return Ok(Some(Child::Directory(segments)));
        }
        if !metadata.is_file() {
            return Ok(None);
        }

        let mode = metadata.permissions().mode();
        let reported = fs::read(&path).ok();

        Ok(proc_sys_entry::classify(&segments, mode, reported.as_deref())?.map(Child::Parameter))
    }

    /// Where a name spelled as segments lives under this root.
    fn path_of(&self, segments: &[String]) -> PathBuf {
        segments
            .iter()
            .fold(self.root.clone(), |path, segment| path.join(segment))
    }
}

/// What the walk found inside a directory.
///
/// A named alternative rather than two collections, so that the walk's one loop
/// says what it does with each kind instead of appending to whichever list.
enum Child {
    Directory(Vec<String>),
    Parameter((SysctlKey, SysctlValue)),
}

impl Default for ProcSys {
    fn default() -> Self {
        Self::new()
    }
}
