//! Why a node, or one of its surfaces, was not read.

/// Why a node or a surface was not read, in words an operator can act on.
///
/// **One type for every way a node goes unread**, its settings, its listeners or its answer,
/// so the facet reports it one way wherever it arose, and never softened into an empty answer:
/// empty settings would read as a node on every default, and no listeners as a node serving
/// nothing.
///
/// **Two kinds, rendered apart.** A failure is rastro supporting the node and not managing to
/// read it, the facet's `error`. Not read is the box's own state keeping rastro out: security
/// switched on and no credential given, a node with no master, a release below 7. Nothing there
/// is broken, so it is not an error, and it is still marked as what the run could not see.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct Unread {
    reason: String,
    not_read: bool,
}

impl Unread {
    /// rastro could not read what it supports.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            not_read: false,
        }
    }

    /// The box's state keeps rastro from reading it.
    pub fn not_read(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            not_read: true,
        }
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn is_not_read(&self) -> bool {
        self.not_read
    }
}
