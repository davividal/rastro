//! Which trees the walker reads, which it only measures, and which it stops at.

use std::collections::BTreeMap;

use rastro_collector::{
    AbsolutePath, CollectionError, FacetName, FilesystemClaim, Observation, WalkedTree,
};

use crate::collectors::filesystem::model::PolicyRule;
use crate::collectors::filesystem::value_objects::{ContentPolicy, DigestAlgorithm};

/// The trees whose content changes on an idle host without its meaning changing.
///
/// Measured rather than guessed: on an idle Debian 12 the only entries that moved in
/// ninety seconds were two journals and the timesync clock. `/var/log/journal` needs no
/// entry of its own because `/var/log` already covers it.
///
/// Short on purpose. Every entry is a place rastro stops being able to report a content
/// change, so the list earns each line by the noise it removes, and it is a noise
/// instrument rather than a performance one: these trees are a seventh of the bytes on
/// that box, while `/usr` is two thirds of them and is the tree a replaced binary hides
/// in.
///
/// They are `Churns` rather than merely unhashed, because leaving their stamps and sizes
/// alone left them producing the very noise the list exists to remove: two journals and
/// the timesync clock were still in the diff of the reference cycle on mtime alone, and
/// `/var/cache` on size and inode.
const CHURNS_WITHOUT_MEANING: [&str; 6] = [
    "/tmp",
    "/var/tmp",
    "/var/log",
    "/var/cache",
    "/var/lib/systemd/timesync",
    "/var/lib/systemd/random-seed",
];

/// What the walker does with each tree it walks.
///
/// An ordered question answered by an unordered table: the rule that applies to a path
/// is the most specific one containing it, so a table is a set of decisions rather than
/// a sequence, and two tables with the same rules in a different order behave the same.
/// The alternative, first match wins, makes a config's meaning depend on the order its
/// lines happen to be in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkPolicy {
    rules: Vec<PolicyRule>,
}

impl WalkPolicy {
    /// Reads a table, or rejects one that cannot answer for every path.
    ///
    /// Two refusals, both because the alternative is a silent wrong answer rather than
    /// a failure: a tree named twice has no most-specific rule, and a table without the
    /// root has paths it cannot answer for at all.
    pub fn new(rules: Vec<PolicyRule>) -> Result<Self, CollectionError> {
        for (position, rule) in rules.iter().enumerate() {
            if rules[..position]
                .iter()
                .any(|earlier| earlier.tree == rule.tree)
            {
                return Err(CollectionError::new(format!(
                    "the policy table names {:?} twice, so there is no most specific rule \
                     for what is inside it",
                    rule.tree.as_str()
                )));
            }
        }

        if !rules.iter().any(|rule| rule.tree.is_root()) {
            return Err(CollectionError::new(
                "the policy table has no rule for /, so it cannot answer for every path \
                 the walk reaches"
                    .to_owned(),
            ));
        }

        Ok(Self { rules })
    }

    /// The table rastro ships with: hash everything, then step back from the trees that
    /// churn without meaning.
    ///
    /// Hashing is the default because config can only narrow. A table that had to name
    /// the trees worth hashing would be an inclusion list, and an operator can only
    /// include what they already knew to look for, which is the premise this tool
    /// rejects.
    pub fn built_in() -> Self {
        let mut rules = vec![Self::shipped_rule(
            "/",
            ContentPolicy::Hashed(DigestAlgorithm::Sha256),
        )];

        rules.extend(
            CHURNS_WITHOUT_MEANING
                .iter()
                .map(|tree| Self::shipped_rule(tree, ContentPolicy::Churns)),
        );

        Self::new(rules).expect("the built-in table names each tree once and covers /")
    }

    /// The same table, with one collector's claims folded in.
    ///
    /// **A conflict fails rather than resolving.** Two rules for one tree leave no most
    /// specific answer, and every way of picking a winner would be rastro deciding for the
    /// operator which of two collectors was right about a tree neither of them should have
    /// been arguing over. It is a bug in a collector pair, so it reads as one: the walk
    /// fails, loudly, naming both claimants and the tree.
    ///
    /// A claim that merely repeats what the shipped table already says is still a conflict.
    /// Agreeing by accident is not agreement, and the next release moving one of the two
    /// would turn a silent duplicate into a silent disagreement.
    pub fn claimed(
        self,
        claimant: &FacetName,
        claims: &[FilesystemClaim],
    ) -> Result<Self, CollectionError> {
        let mut rules = self.rules;

        for claim in claims {
            if let Some(existing) = rules.iter().find(|rule| &rule.tree == claim.tree()) {
                return Err(CollectionError::new(format!(
                    "{:?} is claimed by {} and already ruled by {}, so no rule for it is \
                     the most specific one",
                    claim.tree().as_str(),
                    claimant.as_str(),
                    existing.claimant.as_str()
                )));
            }

            rules.push(PolicyRule {
                tree: claim.tree().clone(),
                content: ContentPolicy::from(claim.reading()),
                claimant: claimant.clone(),
            });
        }

        Self::new(rules)
    }

    /// What to do with a path, according to the most specific tree that contains it.
    ///
    /// Total, because [`Self::new`] guarantees a rule for the root. Ties cannot happen:
    /// every matching tree is an ancestor of the same path, so no two of them share a
    /// depth once each tree appears only once.
    pub fn policy_for(&self, path: &AbsolutePath) -> &ContentPolicy {
        &self
            .rules
            .iter()
            .filter(|rule| rule.tree.contains(path))
            .max_by_key(|rule| rule.tree.depth())
            .expect("a rule for /, which the constructor requires")
            .content
    }

    pub fn rules(&self) -> &[PolicyRule] {
        &self.rules
    }

    /// A rule rastro ships, named by the tree it governs.
    fn shipped_rule(tree: &str, content: ContentPolicy) -> PolicyRule {
        PolicyRule::shipped(
            WalkedTree::new(tree).expect("a built-in tree is an absolute path"),
            content,
        )
    }
}

impl From<&WalkPolicy> for Observation {
    /// The effective table, keyed by tree.
    ///
    /// Keyed rather than listed for the reason every other keyed facet gives: a tree
    /// appears once by construction, so the key loses nothing and removes the ordering
    /// churn a list would carry whenever a claimant came or went. `BTreeMap` because the
    /// shape is open, so the order is sorted rather than declared.
    fn from(policy: &WalkPolicy) -> Self {
        let table: BTreeMap<String, Observation> = policy
            .rules()
            .iter()
            .map(|rule| (rule.tree.as_str().to_owned(), Observation::from(rule)))
            .collect();

        Observation::object(table)
    }
}
