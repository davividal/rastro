//! The `nfsd` filesystem, where the kernel server publishes how it runs.

use std::path::{Path, PathBuf};

use super::kernel_file::read_value;
use crate::collectors::nfs::model::ServerRuntime;

/// Where Debian's `proc-fs-nfsd.mount` puts it.
const NFSD_FILESYSTEM: &str = "/proc/fs/nfsd";

/// The files that are settings, read by name.
///
/// Named, because most of this filesystem is not state: `clients/` and the `*_stats` files move
/// with traffic, `filehandle` and `unlock_*` are commands, and `exports` is a cache. Reading a
/// setting was measured to leave the server as it was, on Debian 12.
const SETTINGS: [&str; 9] = [
    "max_block_size",
    "max_connections",
    "nfsv4gracetime",
    "nfsv4leasetime",
    "nfsv4recoverydir",
    "pool_threads",
    "portlist",
    "threads",
    "versions",
];

/// The file that marks it as mounted: the kernel creates it with the rest.
const MOUNTED_MARKER: &str = "threads";

/// The kernel server's filesystem, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsdFilesystem {
    root: PathBuf,
}

impl NfsdFilesystem {
    pub fn new() -> Self {
        Self::at(NFSD_FILESYSTEM)
    }

    pub fn at(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the filesystem is mounted, which is what makes the server readable at all.
    ///
    /// Not mounted by rastro when it is not: mounting it is a change to the host.
    pub fn is_mounted(&self) -> bool {
        self.root.join(MOUNTED_MARKER).exists()
    }

    /// Every setting, each failing alone.
    pub fn runtime(&self) -> ServerRuntime {
        ServerRuntime::new(
            SETTINGS
                .iter()
                .map(|name| ((*name).to_owned(), read_value(&self.root.join(name)))),
        )
    }
}

impl Default for NfsdFilesystem {
    fn default() -> Self {
        Self::new()
    }
}
