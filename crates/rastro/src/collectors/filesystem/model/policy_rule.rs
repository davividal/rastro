//! One tree, what the walker does with it, and who decided.

use rastro_collector::{FacetName, Observation, WalkedTree};

use crate::collectors::filesystem::value_objects::ContentPolicy;

/// A policy decision about a subtree, and the facet that made it.
///
/// A plain aggregate, because every field is already a validated value and the invariant
/// that matters is about the table as a whole: no tree may appear twice, and the root must
/// be covered. That belongs to [`WalkPolicy`](super::WalkPolicy), which is the only type
/// able to see the other rules.
///
/// `claimant` is the facet that asked, and it is never absent: rastro's own shipped rules
/// name the `filesystem` facet. Without it a reader of a tree with no digests cannot tell a
/// shipped decision from a collector's claim, and the one thing a sealed tree owes them is
/// who removed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRule {
    pub tree: WalkedTree,
    pub content: ContentPolicy,
    pub claimant: FacetName,
}

impl PolicyRule {
    /// A rule rastro ships, attributed to the facet whose walk it governs.
    ///
    /// The only claimant that is not a claim: `filesystem` decided it itself. Named here so
    /// the table and a test spell "shipped" once rather than repeating the facet name at
    /// every rule.
    pub fn shipped(tree: WalkedTree, content: ContentPolicy) -> Self {
        Self {
            tree,
            content,
            claimant: FacetName::new("filesystem").expect("`filesystem` is a legal facet name"),
        }
    }
}

impl From<&PolicyRule> for Observation {
    /// How the effective table renders, one object per rule.
    ///
    /// The tree is the key the table is built under, so it is not repeated here.
    fn from(rule: &PolicyRule) -> Self {
        Observation::object([
            ("claimed_by", Observation::text(rule.claimant.as_str())),
            ("reading", Observation::text(reading_of(&rule.content))),
        ])
    }
}

/// The word the document records for a policy.
///
/// Spelled here rather than on [`ContentPolicy`] because it is the *document's* vocabulary:
/// `hashed` says nothing about which algorithm, since the digest beside every entry already
/// carries that.
fn reading_of(content: &ContentPolicy) -> &'static str {
    match content {
        ContentPolicy::Hashed(_) => "hashed",
        ContentPolicy::MetadataOnly => "metadata_only",
        ContentPolicy::Churns => "churns",
        ContentPolicy::Sealed => "sealed",
    }
}
