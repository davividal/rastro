//! What this box filters.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! # What this facet does not see
//!
//! **A ruleset written natively with `nft` is invisible here, and that is worth reading
//! before trusting an empty result.** On Debian 12 `iptables` is a compatibility front end
//! over nftables — the binary reports `v1.8.9 (nf_tables)` — so `iptables-save` dumps the
//! tables created *through the iptables interface* and nothing else. A box configured with
//! `nft` directly, or by firewalld, has a filter this facet reports as empty.
//!
//! There is deliberately no `nftables` key holding `null`, because that would read as
//! "rastro looked and found none". rastro does not look: `nft` is absent from the
//! development box, so a parser for its output could not be verified against a real
//! ruleset, and shipping an unverifiable parser for the one facet where being quietly wrong
//! is most dangerous is worse than a documented gap.
//!
//! # What an empty ruleset means
//!
//! It means the box filters nothing, and it is the honest answer rather than a failure to
//! read. `iptables-save` on a box with no tables prints zero bytes and exits successfully,
//! which was measured rather than assumed.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{BackendReport, FirewallChain, FirewallInventory, Ruleset};
pub use source::{FirewallSource, iptables_save};
pub use value_objects::{ChainName, ChainPolicy, FirewallBackend, RuleSpecification, TableName};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use crate::collectors::kernel_residency::KernelResidency;

use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct FirewallCollector {
    name: FacetName,
    identity: CollectorIdentity,
    sources: Vec<FirewallSource>,
}

impl FirewallCollector {
    pub fn new() -> Self {
        Self::reading(FirewallSource::detect_all(&KernelResidency::detect()))
    }

    /// The same collector over sources the caller chose.
    pub fn reading(sources: Vec<FirewallSource>) -> Self {
        Self {
            name: FacetName::new("firewall").expect("`firewall` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("firewall").expect("`firewall` is a legal collector id"),
                CollectorVersion::new("2").expect("`2` is a legal collector version"),
            ),
            sources,
        }
    }
}

impl Default for FirewallCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for FirewallCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present, because the subject is the interfaces rastro can read.
    ///
    /// The same call the packages and repositories facets make, and for the same reason: a
    /// box with no `iptables-save` is not a box that filters nothing, so `absent` would be
    /// a claim rastro cannot support. What it does know goes in the data instead, as a key
    /// per interface with `null` for the ones that are not installed.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let found: Vec<_> = self
            .sources
            .iter()
            .map(|source| (source.backend(), source.read()))
            .collect();

        Ok(Observation::from(&FirewallInventory::new(found)?))
    }
}
