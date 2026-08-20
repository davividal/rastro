//! The `/proc/modules` interface.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::proc_modules_line::ProcModulesLine;
use crate::collectors::modules::model::ModuleTable;

/// Where the kernel publishes its loaded modules.
///
/// Effective state over declared state: `/etc/modules` and the `modules-load.d`
/// drop-ins say what should be loaded, this says what is.
const PROC_MODULES: &str = "/proc/modules";

/// Where `/proc` itself is, which is how a kernel with no module support is told apart
/// from a host whose `/proc` was never mounted.
const PROC: &str = "/proc";

/// The entry that proves a procfs is actually mounted rather than merely present.
///
/// `/proc` is a directory in Debian's base layout whether or not anything is mounted on it,
/// so testing the directory answers "does this path exist", not "can rastro see kernel
/// state". In a chroot or a container without procfs that difference is the whole question,
/// and getting it wrong makes the collector assert the kernel has no modules. `self` is
/// provided by procfs and by nothing else.
const PROC_SELF: &str = "self";

/// The kernel's module list as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcModules {
    path: PathBuf,
    procfs: PathBuf,
}

impl ProcModules {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(PROC_MODULES),
            procfs: PathBuf::from(PROC),
        }
    }

    pub fn at(path: impl Into<PathBuf>, procfs: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            procfs: procfs.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where the interface's filesystem is expected to be mounted.
    pub fn filesystem(&self) -> &Path {
        &self.procfs
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Whether the interface's filesystem is mounted at all.
    ///
    /// The distinction this draws is the whole reason it exists: no `/proc/modules` on a
    /// mounted `/proc` means a kernel built without module support, which genuinely has no
    /// modules, while no procfs at all means rastro cannot see kernel state and must say so
    /// instead of reporting an empty truth.
    pub fn filesystem_is_mounted(&self) -> bool {
        self.procfs.join(PROC_SELF).exists()
    }

    /// Reads the interface and translates it into the model.
    pub fn read(&self) -> Result<ModuleTable, CollectionError> {
        let table = fs::read_to_string(&self.path).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", self.path.display()))
        })?;

        Self::parse(&table)
    }

    /// Translates the interface's text into the model.
    ///
    /// Separate from [`Self::read`] so the whole grammar is exercised from a fixture,
    /// with no `/proc` to read from.
    pub fn parse(table: &str) -> Result<ModuleTable, CollectionError> {
        let entries = table
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| ProcModulesLine::parse(line)?.to_entry())
            .collect::<Result<Vec<_>, CollectionError>>()?;

        ModuleTable::new(entries)
    }
}

impl Default for ProcModules {
    fn default() -> Self {
        Self::new()
    }
}
