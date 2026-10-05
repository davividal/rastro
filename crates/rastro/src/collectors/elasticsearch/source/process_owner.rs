//! The account a process runs as, as the kernel says in `/proc/<pid>/status`.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// A process's effective user id and every group it acts with, mapped into rastro's own user
/// namespace by the kernel, so a container's account is the host's id for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOwner {
    uid: u32,
    groups: BTreeSet<u32>,
}

impl ProcessOwner {
    /// The account `process_id` runs as, or nothing where its status cannot be read.
    pub fn of_in(proc: &Path, process_id: u32) -> Option<Self> {
        let status = fs::read_to_string(proc.join(process_id.to_string()).join("status")).ok()?;
        let field = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
                .map(|values| {
                    values
                        .split_whitespace()
                        .map(str::parse::<u32>)
                        .collect::<Result<Vec<u32>, _>>()
                })
        };
        // The second of `Uid:` and `Gid:` is the effective one, which the kernel checks access by.
        let uid = *field("Uid")?.ok()?.get(1)?;
        let gid = *field("Gid")?.ok()?.get(1)?;
        let mut groups: BTreeSet<u32> = field("Groups")?.ok()?.into_iter().collect();
        groups.insert(gid);

        Some(Self { uid, groups })
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    /// Whether the process may write in the directory `metadata` describes, by its owner's or
    /// its group's bits as the kernel picks them. **Never by the world's**: `/tmp` is writable by
    /// every account, a node's among them, and is no store.
    pub fn can_write(&self, metadata: &fs::Metadata) -> bool {
        const OWNER_WRITE: u32 = 0o200;
        const GROUP_WRITE: u32 = 0o020;

        match (
            metadata.uid() == self.uid,
            self.groups.contains(&metadata.gid()),
        ) {
            (true, _) => metadata.mode() & OWNER_WRITE != 0,
            (false, true) => metadata.mode() & GROUP_WRITE != 0,
            (false, false) => false,
        }
    }
}
