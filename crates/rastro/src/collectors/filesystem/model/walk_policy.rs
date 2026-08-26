//! Which trees the walker reads, and which it only measures.

use rastro_collector::{AbsolutePath, CollectionError};

use crate::collectors::filesystem::model::PolicyRule;
use crate::collectors::filesystem::value_objects::{ContentPolicy, DigestAlgorithm, WalkedTree};

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
        let mut rules = vec![Self::rule(
            "/",
            ContentPolicy::Hashed(DigestAlgorithm::Sha256),
        )];

        rules.extend(
            CHURNS_WITHOUT_MEANING
                .iter()
                .map(|tree| Self::rule(tree, ContentPolicy::MetadataOnly)),
        );

        Self::new(rules).expect("the built-in table names each tree once and covers /")
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

    fn rule(tree: &str, content: ContentPolicy) -> PolicyRule {
        PolicyRule {
            tree: WalkedTree::new(tree).expect("a built-in tree is an absolute path"),
            content,
        }
    }
}
