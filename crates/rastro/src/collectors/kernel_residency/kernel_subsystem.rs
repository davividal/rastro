//! A kernel subsystem rastro may want to read.

/// A subsystem named the two ways the kernel names it.
///
/// Both are needed because neither alone answers the question. `/proc/modules` lists the
/// module name and says nothing about a kernel that was built with the subsystem compiled
/// in; `/boot/config-<release>` lists the configuration symbol and says nothing about what
/// is loaded right now. The mapping between the two is not mechanical, which is why it is
/// declared per subsystem rather than derived: `nf_tables` comes from `CONFIG_NF_TABLES`,
/// but `ip_tables` comes from `CONFIG_IP_NF_IPTABLES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KernelSubsystem {
    module: &'static str,
    config_symbol: &'static str,
}

impl KernelSubsystem {
    pub const fn new(module: &'static str, config_symbol: &'static str) -> Self {
        Self {
            module,
            config_symbol,
        }
    }

    pub fn module(&self) -> &'static str {
        self.module
    }

    pub fn config_symbol(&self) -> &'static str {
        self.config_symbol
    }
}
