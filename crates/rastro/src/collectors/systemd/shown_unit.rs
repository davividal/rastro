//! One unit's properties, as `systemctl show` reported them.

use std::collections::BTreeMap;

use rastro_collector::EnvironmentVariableName;

use super::exec_start::ExecStart;

/// What one group of the property dump amounts to.
///
/// A named type rather than a pair, because the dump grew a second property and a caller
/// reading `shown.1` would say nothing about which. Both collectors that read the dump take
/// this, so a third property later reaches them without changing either signature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShownUnit {
    /// What systemd will run, resolved through every drop-in, in the order it runs them.
    pub exec_start: Vec<ExecStart>,
    /// The variables the unit declares, keyed by name so a diff has one order.
    ///
    /// **This is what the unit file says, not what the process gets.** systemd reads an
    /// `EnvironmentFile=` at exec time rather than at load time, so what those files
    /// contribute is absent here — measured against systemd 257, where a variable set only
    /// in the file did not appear on this line. The files themselves are named separately,
    /// which is the only reason reporting them is worth anything.
    pub environment: BTreeMap<EnvironmentVariableName, String>,
}
