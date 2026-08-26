//! One tree, and what the walker does with it.

use crate::collectors::filesystem::value_objects::{ContentPolicy, WalkedTree};

/// A policy decision about a subtree.
///
/// A plain pair, because both fields are already validated values and the invariant
/// that matters is about the table as a whole: no tree may appear twice, and the root
/// must be covered. That belongs to [`WalkPolicy`](super::WalkPolicy), which is the
/// only type able to see the other rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRule {
    pub tree: WalkedTree,
    pub content: ContentPolicy,
}
