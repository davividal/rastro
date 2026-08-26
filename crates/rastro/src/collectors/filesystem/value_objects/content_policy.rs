//! What the walker does with a file's content.

use crate::collectors::filesystem::value_objects::DigestAlgorithm;

/// Whether a tree's files are read, or known by their metadata alone.
///
/// The two questions a fingerprint can ask of a file are separable, and this is the
/// expensive one: reading content costs the whole file, while `stat` costs a syscall.
/// Splitting them is what lets a tree that churns without meaning stay in the document
/// as an entry while its content stops producing noise.
///
/// Absence of a digest is therefore a decision, not a gap, and a reader can only tell
/// the two apart because the effective table travels in the envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentPolicy {
    Hashed(DigestAlgorithm),

    /// Recorded, but never opened.
    MetadataOnly,
}
