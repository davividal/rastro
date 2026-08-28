//! One tree a collector owns, and how it asks for it to be read.

use crate::claims::ClaimedReading;
use crate::value_objects::WalkedTree;

/// A collector's claim over one tree.
///
/// Built through the three named constructors rather than as a literal, so a call site
/// reads as the sentence the claimant means: `FilesystemClaim::sealed(data_directory)`.
///
/// It carries no claimant. Who claimed what is recorded by whoever gathers the claims,
/// because a collector naming itself in its own claim could name somebody else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemClaim {
    tree: WalkedTree,
    reading: ClaimedReading,
}

impl FilesystemClaim {
    /// Stat this tree, open nothing in it.
    pub fn metadata_only(tree: WalkedTree) -> Self {
        Self::of(tree, ClaimedReading::MetadataOnly)
    }

    /// Stat this tree, open nothing, and treat the attributes that move as volatile.
    pub fn churns(tree: WalkedTree) -> Self {
        Self::of(tree, ClaimedReading::Churns)
    }

    /// Record this tree's own directory and go no further into it.
    pub fn sealed(tree: WalkedTree) -> Self {
        Self::of(tree, ClaimedReading::Sealed)
    }

    pub fn tree(&self) -> &WalkedTree {
        &self.tree
    }

    pub fn reading(&self) -> ClaimedReading {
        self.reading
    }

    fn of(tree: WalkedTree, reading: ClaimedReading) -> Self {
        Self { tree, reading }
    }
}
