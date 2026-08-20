//! One chain, and the rules in it.

use rastro_collector::Observation;

use crate::collectors::firewall::value_objects::{ChainPolicy, RuleSpecification};

/// A chain as rastro means it.
///
/// The chain's name is not a field here, because it is the key this is filed under.
///
/// **The rules keep the tool's order, and that is not a stylistic choice.** A packet is
/// tested against a chain's rules in order and stops at the first that matches, so the
/// order *is* the meaning: moving a `DROP` above an `ACCEPT` reverses what the chain does
/// while changing no rule at all. This is the same reasoning that keeps the mount table in
/// the kernel's order, and the opposite of the sorting every other list in this project
/// gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallChain {
    /// Absent for a user-defined chain, which falls through to its caller instead.
    pub policy: Option<ChainPolicy>,
    pub rules: Vec<RuleSpecification>,
}

impl From<&FirewallChain> for Observation {
    fn from(chain: &FirewallChain) -> Self {
        Observation::object([
            (
                "policy",
                chain
                    .policy
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "rules",
                Observation::list(chain.rules.iter().map(Observation::from)),
            ),
        ])
    }
}
