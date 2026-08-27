//! Everything the walk found.

use rastro_collector::Observation;

use crate::collectors::filesystem::model::FileEntry;

/// The entries of one walk, ordered by path.
///
/// Sorted here rather than left to the filesystem. `read_dir` returns entries in whatever
/// order the filesystem keeps them, which for ext4 is a hash order that has nothing to do
/// with the names and is not stable across a rebuild. Ordering is part of the output
/// contract, so it is imposed where the entries are collected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemInventory {
    entries: Vec<FileEntry>,
}

impl FilesystemInventory {
    pub fn new(mut entries: Vec<FileEntry>) -> Self {
        entries.sort_by(|left, right| left.path.cmp(&right.path));

        Self { entries }
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }
}

impl From<&FilesystemInventory> for Observation {
    fn from(inventory: &FilesystemInventory) -> Self {
        Observation::object(
            inventory
                .entries()
                .iter()
                .map(|entry| (entry.path.as_str(), Observation::from(entry))),
        )
    }
}
