//! Which view of a document is being asked for.

/// Which part of a fingerprint is wanted.
///
/// Not a format. JSON, YAML or anything else renders either view; this says
/// *what is in* the document, not what it looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Everything the collectors observed.
    Complete,
    /// Volatile values omitted, so that two runs on an unchanged host produce
    /// byte-identical output. The view the determinism contract is about, and
    /// the one worth diffing.
    Diffable,
}
