//! The server binaries installed in the system directories.

use std::collections::BTreeSet;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::redis::value_objects::ServerKind;

/// Which server families have a binary on this box.
///
/// **Located, never run.** Nothing here executes a server: starting one to ask its version
/// would be a daemon rastro started. The location is the whole fact, and it is what makes an
/// installed-and-stopped box `present` rather than `absent`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledServers {
    kinds: BTreeSet<ServerKind>,
}

impl InstalledServers {
    /// The families whose server binary is in a system directory on this box.
    pub fn located() -> Self {
        Self::new(
            ServerKind::ALL
                .into_iter()
                .filter(|kind| CanonicalTool::located(kind.program()).is_some()),
        )
    }

    /// The same over families the caller names.
    pub fn new(kinds: impl IntoIterator<Item = ServerKind>) -> Self {
        Self {
            kinds: kinds.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// The families, in a fixed order.
    pub fn kinds(&self) -> &BTreeSet<ServerKind> {
        &self.kinds
    }
}
