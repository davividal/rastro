//! How a file's content is reduced to something comparable.

/// The digest a hashed tree is read with.
///
/// One variant, and it is still an enum rather than an assumption, because the
/// algorithm has to reach the document. Two fingerprints hashed differently cannot be
/// diffed against each other at all, so a reader holding a `before.json` needs to see
/// which algorithm produced it rather than infer the one that happened to be current.
///
/// Adding a variant is deliberately a recompile. Every addition changes what a leaf
/// looks like, which makes it a change to the output contract, and an exhaustive match
/// is what makes the compiler name every place that has to be updated with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DigestAlgorithm {
    Sha256,
}

impl DigestAlgorithm {
    /// The name the document records, which is the reason this type is not a bool.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
        }
    }
}
