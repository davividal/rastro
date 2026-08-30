//! Everything the walk found.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::filesystem::model::{FileEntry, UnreadablePath};
use crate::collectors::filesystem::value_objects::Detail;

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
///
/// **A path is described or refused, never both.** Two walks reaching one path and
/// disagreeing about whether it could be read is the same contradiction as disagreeing about
/// its mode, one level out, so it is refused for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemInventory {
    entries: Vec<FileEntry>,
    unreadable: Vec<UnreadablePath>,
}

impl FilesystemInventory {
    pub fn new(
        mut entries: Vec<FileEntry>,
        mut unreadable: Vec<UnreadablePath>,
    ) -> Result<Self, CollectionError> {
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        unreadable.sort_by(|left, right| left.path.cmp(&right.path));

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
        unreadable.dedup();

        if let Some(contested) = unreadable
            .iter()
            .find(|refused| {
                entries
                    .binary_search_by(|entry| entry.path.cmp(&refused.path))
                    .is_ok()
            })
            .map(|refused| refused.path.as_str())
        {
            return Err(CollectionError::new(format!(
                "{contested:?} was both described and refused, so the walk did not see one \
                 moment of this host"
            )));
        }

        Ok(Self {
            entries,
            unreadable,
        })
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    pub fn unreadable(&self) -> &[UnreadablePath] {
        &self.unreadable
    }

    /// Everything found by several walks, as one inventory.
    pub fn merged(
        inventories: impl IntoIterator<Item = FilesystemInventory>,
    ) -> Result<Self, CollectionError> {
        let mut entries = Vec::new();
        let mut unreadable = Vec::new();

        for inventory in inventories {
            entries.extend(inventory.entries);
            unreadable.extend(inventory.unreadable);
        }

        Self::new(entries, unreadable)
    }

    /// Every path this walk reached, keyed by path.
    ///
    /// Both lists under one key each, because a reader looks a path up rather than asking
    /// which of two collections it landed in. A described path carries what `detail` asks for;
    /// a refused one carries the reason either way, since there is no less of it to record.
    pub fn observation(&self, detail: Detail) -> Observation {
        let described = self
            .entries()
            .iter()
            .map(|entry| (entry.path.as_str(), entry.observation(detail)));
        let refused = self
            .unreadable()
            .iter()
            .map(|refused| (refused.path.as_str(), Observation::from(refused)));

        Observation::object(described.chain(refused))
    }
}
