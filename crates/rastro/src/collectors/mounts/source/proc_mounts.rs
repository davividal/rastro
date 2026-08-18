//! The `/proc/mounts` interface.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::proc_mounts_line::ProcMountsLine;
use crate::collectors::mounts::model::{Mount, MountTable};

/// Where the kernel publishes the mount table.
///
/// Effective state over declared state: `/etc/fstab` says what someone intended to
/// mount, this says what is mounted.
const PROC_MOUNTS: &str = "/proc/mounts";

/// The kernel's mount table as a source rastro can read.
///
/// It owns its location, so the collector above it never mentions a path and the
/// interface can be pointed elsewhere for a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcMounts {
    path: PathBuf,
}

impl ProcMounts {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(PROC_MOUNTS),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the interface and translates it into the model.
    pub fn read(&self) -> Result<MountTable, CollectionError> {
        let table = fs::read_to_string(&self.path).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", self.path.display()))
        })?;

        Self::parse(&table)
    }

    /// Translates the interface's text into the model.
    ///
    /// Separate from [`Self::read`] so the whole grammar is exercised from a fixture,
    /// with no `/proc` to read from.
    ///
    /// Blank lines are skipped because they carry no mount; a line with content that
    /// does not parse is an error, never a skip.
    pub fn parse(table: &str) -> Result<MountTable, CollectionError> {
        let mounts = table
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| ProcMountsLine::parse(line)?.to_mount())
            .collect::<Result<Vec<Mount>, CollectionError>>()?;

        Ok(MountTable::new(mounts))
    }
}

impl Default for ProcMounts {
    fn default() -> Self {
        Self::new()
    }
}
