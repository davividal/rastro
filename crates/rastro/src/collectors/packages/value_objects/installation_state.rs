//! What is actually there.

use rastro_collector::{CollectionError, NonEmptyText};

/// How far a package actually got: `installed`, `config-files`, `half-configured`,
/// `not-installed`, and the rest of dpkg's set.
///
/// The gap between this and the [`SelectionState`](super::selection_state::SelectionState)
/// is the interesting part: wanted `install` but state `half-configured` is a box that
/// failed mid-upgrade.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstallationState(NonEmptyText);

impl InstallationState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "installation state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
