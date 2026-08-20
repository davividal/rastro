//! One rule, as the tool spells it.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The body of one rule, kept verbatim.
///
/// **Deliberately not parsed into matches and a target.** A rule specification is an open
/// grammar: each of the several dozen iptables match extensions adds its own options, and
/// they combine freely. Modelling it would mean reimplementing the extension set, and being
/// wrong the first time a box used one rastro had not heard of — which, for a firewall, is
/// the worst place in a fingerprint to be quietly incomplete.
///
/// Kept as the tool wrote it, a rule diffs exactly and can be pasted straight back into
/// `iptables`. What is given up is diffing *within* a rule: changing one port shows as one
/// rule replaced rather than as one field changed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleSpecification(NonEmptyText);

impl RuleSpecification {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "rule specification")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&RuleSpecification> for Observation {
    fn from(value: &RuleSpecification) -> Self {
        Observation::text(value.as_str())
    }
}
