//! How much of a claimed tree the walk reads.

/// The three steps back from the default a claim can ask for.
///
/// Each reports strictly less than the one before, and none of them can be spelled as
/// "hash this": the vocabulary is what makes a claim a narrowing rather than a
/// negotiation.
///
/// The distinction between the first two is not academic. A media store or a data
/// directory too large to hash still has meaningful stamps, because a new file arriving is
/// exactly the signal an operator wants; a cluster's WAL or a journal moves its stamps on
/// every write, and there the same fields are noise. One claim cannot serve both, so the
/// claimant says which it owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimedReading {
    /// Stat every entry, open none of them.
    MetadataOnly,

    /// As [`Self::MetadataOnly`], and the attributes that move on their own are volatile.
    ///
    /// Size, inode and both stamps. What survives is presence, kind, permissions and
    /// ownership, which is what an operator can act on in a tree that writes to itself.
    Churns,

    /// Record the tree's own directory and do not descend into it.
    ///
    /// The only claim that changes what the document *contains* rather than how it reads,
    /// which is why the root entry stays and the effective table names the claimant:
    /// nothing disappears without saying who removed it and why.
    Sealed,
}
