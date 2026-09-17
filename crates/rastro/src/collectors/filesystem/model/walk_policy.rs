//! Which trees the walker reads, which it only measures, and which it stops at.

use std::collections::BTreeMap;

use rastro_collector::{
    AbsolutePath, CollectionError, FacetName, FilesystemClaim, Observation, WalkedTree,
};

use crate::collectors::filesystem::model::PolicyRule;
use crate::collectors::filesystem::value_objects::{Claimant, ContentPolicy};

/// The trees whose content changes on an idle host without its meaning changing.
///
/// Measured rather than guessed: on an idle Debian 12 the only entries that moved in
/// ninety seconds were two journals and the timesync clock. `/var/log/journal` needs no
/// entry of its own because `/var/log` already covers it.
///
/// Short on purpose, and it survives on its noise argument alone. It never was a
/// performance instrument, and now that the walk opens no file it cannot be mistaken for
/// one: what each line still buys is the stamps, the size and the inode going volatile,
/// which is the difference between a quiet diff and one carrying two journals every run.
/// Do not retire the list on the grounds that nothing is hashed any more.
///
/// They are `Churns` rather than merely unread, because leaving their stamps and sizes
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

    /// The table rastro ships with: stat everything, open nothing, then step back from
    /// the trees whose stamps churn without meaning.
    ///
    /// **What narrowed is the reading, not the scope.** The walk is still total over every
    /// mount that holds files, and every path it reaches is still in the document, so no
    /// state surface left it and this is not the inclusion list the earlier default was
    /// written to avoid. A tree the table says nothing about loses one attribute, not its
    /// existence.
    ///
    /// Hashing everything was measured on a production PostgreSQL host: 84 GB read and
    /// climbing, 10.4M read syscalls, and a run killed at 51 minutes having produced
    /// nothing. What it bought over stat was detection of a content change that preserved
    /// every attribute, which needs `touch -r` *and* a moved clock, because ctime has no
    /// userspace setter. That is deliberate evasion, and this tool is not an intrusion
    /// detector. An ordinary write moves mtime, ctime and usually size, so a change at any
    /// path is still detected here.
    ///
    /// Opening no file is also what makes the walk leave nothing behind: no atime moves,
    /// and no file data enters the page cache, so rastro cannot evict the working set of
    /// the database it is fingerprinting.
    ///
    /// Content hashing returns as an opt-in collector over trees the operator names, which
    /// is where the cost can be consented to rather than discovered.
    pub fn built_in() -> Self {
        let mut rules = vec![Self::shipped_rule("/", ContentPolicy::MetadataOnly)];

        rules.extend(
            CHURNS_WITHOUT_MEANING
                .iter()
                .map(|tree| Self::shipped_rule(tree, ContentPolicy::Churns)),
        );

        Self::new(rules).expect("the built-in table names each tree once and covers /")
    }

    /// The same table, with one collector's claims folded in.
    ///
    /// **A tree more than one claim names is sealed, and every claimant is kept.** Two rules
    /// for one tree leave no most specific answer, and every way of picking a winner would be
    /// rastro deciding for the operator which claim was right about a tree none of them should
    /// have been arguing over.
    ///
    /// Sealing is not rastro settling that argument. It is rastro declining to walk into a
    /// tree it cannot account for, which is the one answer that needs no winner. What the
    /// claims asked for stops applying, agreeing claims included: two clusters registered on
    /// one data directory agree on the reading and are still a box in a state nobody intended,
    /// and rastro cannot tell which of them owns the directory, nor whether the one that was
    /// down while the walk ran is about to come back up.
    ///
    /// rastro's own shipped rules are claimants like any other, so a claim that repeats one
    /// contests it. The root is a tree like any other too: a special case there would buy a
    /// branch and nothing else, since a sealed root still leaves an entry that says what
    /// happened.
    ///
    /// **Nothing here fails.** The tree is reported as contested, by
    /// [`Self::contested`], and an operator's rule replaces the seal outright, so a tree
    /// rastro declined to walk is never a dead end.
    pub fn claimed(
        self,
        claimant: &FacetName,
        claims: &[FilesystemClaim],
    ) -> Result<Self, CollectionError> {
        let mut rules = self.rules;

        for claim in claims {
            let claimant = claimant_of(claimant, claim);

            match rules.iter_mut().find(|rule| &rule.tree == claim.tree()) {
                Some(existing) => existing.contested_by(claimant),
                None => rules.push(PolicyRule {
                    tree: claim.tree().clone(),
                    content: ContentPolicy::from(claim.reading()),
                    claimants: vec![claimant],
                }),
            }
        }

        Self::new(rules)
    }

    /// The trees more than one claim named, which the walk seals and reports.
    pub fn contested(&self) -> impl Iterator<Item = &PolicyRule> {
        self.rules.iter().filter(|rule| rule.is_contested())
    }

    /// The same table, with the operator's own rules folded in over everything else.
    ///
    /// **An operator's rule beats a collector's claim, and a shipped one.** A claim is rastro's
    /// reckoning about a tree from the outside — resolved from the host, but still a guess about
    /// what matters in it. The operator knows their box. So where a claim and a config name one
    /// tree this replaces rather than refuses, and the effective table records `config` as the
    /// claimant so the change is declared rather than silent.
    ///
    /// That is a different resolution from [`Self::claimed`], and deliberately: claims naming
    /// one tree have no winner to pick, so the tree is sealed and every claimant reported. An
    /// operator and a collector naming one tree do have an obvious winner. So a config rule
    /// replaces whatever it names, a contested seal included, and the tree stops being
    /// contested rather than staying sealed with a note: the operator has said what to do
    /// with it, which is the whole thing rastro was missing.
    ///
    /// **A config still cannot widen the walk.** Only the three narrowings can be spelled here,
    /// because that is all the config type can hold — there is no `hashed` key. The type is what
    /// enforces it, exactly as `ClaimedReading` enforces it for a claim.
    ///
    /// Two config rules for one tree is still refused: the operator meant one of them and rastro
    /// cannot know which.
    pub fn configured(self, rules: Vec<PolicyRule>) -> Result<Self, CollectionError> {
        let mut overridden: Vec<PolicyRule> = self
            .rules
            .into_iter()
            .filter(|existing| !rules.iter().any(|rule| rule.tree == existing.tree))
            .collect();
        overridden.extend(rules);

        Self::new(overridden)
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

/// Who a claim is filed under: the facet it came from, and the entry of that facet that
/// asked, where the claim named one.
fn claimant_of(facet: &FacetName, claim: &FilesystemClaim) -> Claimant {
    match claim.qualifier() {
        Some(entry) => Claimant::entry(facet.clone(), entry.clone()),
        None => Claimant::facet(facet.clone()),
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
