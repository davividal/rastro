//! A packet-filter interface rastro found, and how to read it.

use rastro_collector::CollectionError;

use super::iptables_save;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::firewall::model::Ruleset;
use crate::collectors::firewall::value_objects::FirewallBackend;

/// One interface present on the host, together with the tool that dumps it.
///
/// A struct rather than the enum the packages and repositories facets use, because unlike
/// those two the interfaces are read *identically*: both `iptables-save` and
/// `ip6tables-save` write the same format, and only the program name differs. An enum whose
/// arms did the same thing would be ceremony.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallSource {
    backend: FirewallBackend,
    tool: CanonicalTool,
}

impl FirewallSource {
    /// The interfaces this host actually has.
    pub fn detect_all() -> Vec<Self> {
        FirewallBackend::ALL
            .into_iter()
            .filter_map(Self::detect)
            .collect()
    }

    fn detect(backend: FirewallBackend) -> Option<Self> {
        CanonicalTool::located(backend.program()).map(|tool| Self::using(backend, tool))
    }

    /// The same over a tool the caller located.
    pub fn using(backend: FirewallBackend, tool: CanonicalTool) -> Self {
        Self { backend, tool }
    }

    pub fn backend(&self) -> FirewallBackend {
        self.backend
    }

    /// Dumps the ruleset.
    ///
    /// No arguments at all, which is the whole point of using `iptables-save` rather than
    /// `iptables -S`: the latter reports one table and would need rastro to know the list of
    /// tables to ask about, while the dump covers every table that exists.
    pub fn read(&self) -> Result<Ruleset, CollectionError> {
        iptables_save::parse(&self.tool.run(&[])?)
    }
}
