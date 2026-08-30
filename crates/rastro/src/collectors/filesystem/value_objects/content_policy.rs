//! What the walker does with a tree it reaches.

use rastro_collector::ClaimedReading;

use crate::collectors::filesystem::value_objects::DigestAlgorithm;

/// How much of a tree the walk reads.
///
/// The two questions a fingerprint can ask of a file are separable, and reading content is
/// the expensive one: it costs the whole file, while `stat` costs a syscall. Splitting them
/// is what lets a tree that churns without meaning stay in the document as an entry while
/// its content stops producing noise.
///
/// Four values, and only the first is an instruction to read. The other three are the
/// steps back a collector's claim can ask for, in the same order
/// [`ClaimedReading`](rastro_collector::ClaimedReading) spells them, because the table and
/// the claim have to mean the same thing. The algorithm rides on `Hashed` and nowhere else,
/// which is what keeps it out of the port: a claim cannot ask for hashing, so it never
/// needs to name how.
///
/// Absence of a digest is therefore a decision, not a gap, and a reader tells the two apart
/// because the effective table travels in the `invocation` facet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentPolicy {
    Hashed(DigestAlgorithm),

    /// Recorded, but never opened.
    MetadataOnly,

    /// Recorded and never opened, with the attributes that move on their own volatile.
    Churns,

    /// The tree's own directory is recorded, and the walk does not descend.
    Sealed,
}

impl ContentPolicy {
    /// Whether an entry under this policy carries attributes that move on their own.
    ///
    /// True for the two policies a claimant asks for when a tree writes to itself. A sealed
    /// tree is included because the only entry it produces is its own directory, whose
    /// stamps move for the contents nothing else is going to report.
    pub fn churns(&self) -> bool {
        matches!(self, Self::Churns | Self::Sealed)
    }

    /// Whether the walk goes on into this tree, or stops at its root.
    pub fn is_descended(&self) -> bool {
        !matches!(self, Self::Sealed)
    }
}

impl From<ClaimedReading> for ContentPolicy {
    /// A claim's vocabulary, in the table's terms.
    ///
    /// Total in one direction only, which is the invariant: every claim maps to a policy,
    /// and no claim maps to `Hashed`.
    fn from(reading: ClaimedReading) -> Self {
        match reading {
            ClaimedReading::MetadataOnly => Self::MetadataOnly,
            ClaimedReading::Churns => Self::Churns,
            ClaimedReading::Sealed => Self::Sealed,
        }
    }
}
