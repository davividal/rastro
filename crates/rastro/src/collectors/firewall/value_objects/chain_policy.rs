//! What happens to a packet no rule matched.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A chain's default policy.
///
/// `ACCEPT` or `DROP` for a built-in chain.
///
/// **The single most consequential value in this facet.** A built-in chain whose policy
/// moves from `ACCEPT` to `DROP` changes what the box does with every packet no rule
/// mentions, and it is one word on one line.
///
/// A user-defined chain has no policy, and `iptables-save` writes `-` in its place. That is
/// recorded as absent rather than as a policy named `-`, because a chain that falls through
/// to its caller is a different thing from one with a default.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChainPolicy(NonEmptyText);

impl ChainPolicy {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "chain policy")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ChainPolicy> for Observation {
    fn from(value: &ChainPolicy) -> Self {
        Observation::text(value.as_str())
    }
}
