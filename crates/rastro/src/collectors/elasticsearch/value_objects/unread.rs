//! Why a node could not be read.

/// Why a node could not be read, in words an operator can act on.
///
/// **One type for every way a node goes unread**, its settings, its listeners or its answer,
/// so the facet reports a failure one way wherever it arose. A node that goes unread is one the
/// dispatch may not ask, and this is never softened into an empty answer: empty settings would
/// read as a node on every default, and no listeners as a node serving nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct Unread {
    reason: String,
}

impl Unread {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}
