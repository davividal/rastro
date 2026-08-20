//! Which interface a ruleset was read through.

/// A packet-filter interface rastro can read.
///
/// **Two, and the absence of a third is the interesting part.** On Debian 12 `iptables`
/// is a compatibility front end over nftables — `iptables --version` says
/// `v1.8.9 (nf_tables)` — so what these two report is the set of tables written *through
/// the iptables interface*. A ruleset written natively with `nft` lives in the same kernel
/// subsystem and does not appear here at all.
///
/// There is deliberately no `Nftables` variant, and that is the opposite of the call the
/// packages and repositories facets make. There, a key with `null` under it says "rastro
/// looked and this manager is not installed". A `nftables: null` here would say rastro
/// looked at the native ruleset and found nothing, which would be untrue: it does not look.
/// The gap is documented on the collector instead, where it cannot be mistaken for an
/// observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FirewallBackend {
    Iptables,
    Ip6tables,
}

impl FirewallBackend {
    /// Every interface rastro knows how to read.
    ///
    /// The same hazard the other two inventories document: a variant added without
    /// extending this list is built and never probed.
    pub const ALL: [Self; 2] = [Self::Iptables, Self::Ip6tables];

    /// The program that dumps this interface's ruleset.
    pub fn program(&self) -> &'static str {
        match self {
            Self::Iptables => "iptables-save",
            Self::Ip6tables => "ip6tables-save",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Iptables => "iptables",
            Self::Ip6tables => "ip6tables",
        }
    }
}
