//! Which interface a ruleset was read through.

use crate::collectors::kernel_residency::KernelSubsystem;

/// The nftables subsystem, which both families share.
///
/// One module serves IPv4 and IPv6, so provoking it for one family provokes it for both.
const NF_TABLES: KernelSubsystem = KernelSubsystem::new("nf_tables", "CONFIG_NF_TABLES");

/// The legacy IPv4 table subsystem.
const IP_TABLES: KernelSubsystem = KernelSubsystem::new("ip_tables", "CONFIG_IP_NF_IPTABLES");

/// The legacy IPv6 table subsystem, which is a separate module from its IPv4 twin.
const IP6_TABLES: KernelSubsystem = KernelSubsystem::new("ip6_tables", "CONFIG_IP6_NF_IPTABLES");

/// A packet-filter interface rastro can read.
///
/// **Four, and the split by implementation is what makes reading them free.** On Debian 12
/// `iptables-save` is a symlink the alternatives system points at `iptables-nft`, so
/// running it opens an nfnetlink socket and the kernel loads `nf_tables`, `nfnetlink` and
/// `libcrc32c` behind it. rastro would then have changed the host it was describing, and a
/// before-and-after pair would blame those three loads on whatever change was under test.
/// That was measured on the development box, not assumed.
///
/// So each implementation is named outright, and each declares the subsystem it would
/// provoke. A backend is read only when that subsystem is already resident, and asking a
/// resident subsystem loads nothing.
///
/// **Legacy and nftables are separate rulesets, not two views of one.** A box can hold
/// tables in both at once and `iptables-legacy-save` reports nothing about the nftables
/// side. Merging them under one key would have to fold two `filter` tables together.
///
/// A ruleset written natively with `nft` still does not appear: these dump what was written
/// *through the iptables interface*. What is new is that an unloaded `nf_tables` is now a
/// statement rather than a silence, because nothing can be holding rules in a subsystem the
/// kernel has not loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FirewallBackend {
    Ip6tablesLegacy,
    Ip6tablesNft,
    IptablesLegacy,
    IptablesNft,
}

impl FirewallBackend {
    /// Every interface rastro knows how to read.
    ///
    /// Declared in the order the keys sort in, so the document's key order and this list
    /// cannot drift apart. The same hazard the other two inventories document: a variant
    /// added without extending this list is built and never probed.
    pub const ALL: [Self; 4] = [
        Self::Ip6tablesLegacy,
        Self::Ip6tablesNft,
        Self::IptablesLegacy,
        Self::IptablesNft,
    ];

    /// The program that dumps this interface's ruleset.
    ///
    /// Never the bare `iptables-save` or `ip6tables-save`: those are the alternatives
    /// symlinks, and which implementation they resolve to is a property of the box rather
    /// than of what rastro asked for.
    pub fn program(&self) -> &'static str {
        match self {
            Self::Ip6tablesLegacy => "ip6tables-legacy-save",
            Self::Ip6tablesNft => "ip6tables-nft-save",
            Self::IptablesLegacy => "iptables-legacy-save",
            Self::IptablesNft => "iptables-nft-save",
        }
    }

    /// The kernel subsystem this interface talks to, and would load if it were absent.
    pub fn subsystem(&self) -> KernelSubsystem {
        match self {
            Self::Ip6tablesLegacy => IP6_TABLES,
            Self::Ip6tablesNft => NF_TABLES,
            Self::IptablesLegacy => IP_TABLES,
            Self::IptablesNft => NF_TABLES,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ip6tablesLegacy => "ip6tables_legacy",
            Self::Ip6tablesNft => "ip6tables_nft",
            Self::IptablesLegacy => "iptables_legacy",
            Self::IptablesNft => "iptables_nft",
        }
    }
}
