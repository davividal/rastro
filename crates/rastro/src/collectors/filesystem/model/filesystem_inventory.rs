//! Everything the walk found.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::filesystem::model::FileEntry;

/// The entries of one or more walks, ordered by path.
///
/// Sorted here rather than left to the filesystem. `read_dir` returns entries in whatever
/// order the filesystem keeps them, which for ext4 is a hash order that has nothing to do
/// with the names and is not stable across a rebuild. Ordering is part of the output
/// contract, so it is imposed where the entries are collected.
///
/// **One path appears once.** A mount point is reached twice when both it and its parent
/// filesystem are walked: `/boot/efi` is an entry of the walk of `/`, which stops there, and
/// the root of its own walk. Both readings are of the same directory and agree, so the
/// duplicate is collapsed. Two readings that *disagree* are refused, because that means the
/// path changed between them and neither describes one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemInventory {
    entries: Vec<FileEntry>,
}

impl FilesystemInventory {
    pub fn new(mut entries: Vec<FileEntry>) -> Result<Self, CollectionError> {
        entries.sort_by(|left, right| left.path.cmp(&right.path));

        if let Some(conflicting) = entries
            .windows(2)
            .find(|pair| pair[0].path == pair[1].path && pair[0] != pair[1])
            .map(|pair| pair[0].path.as_str())
        {
            return Err(CollectionError::new(format!(
                "{conflicting:?} was read twice and differently, so the walk did not see one \
                 moment of this host"
            )));
        }

        entries.dedup();

        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// Everything found by several walks, as one inventory.
    pub fn merged(
        inventories: impl IntoIterator<Item = FilesystemInventory>,
    ) -> Result<Self, CollectionError> {
        Self::new(
            inventories
                .into_iter()
                .flat_map(|inventory| inventory.entries)
                .collect(),
        )
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
