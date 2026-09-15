//! One unit's properties, as `systemctl show` reported them.

use std::collections::BTreeMap;

use rastro_collector::EnvironmentVariableName;

use super::environment_file::EnvironmentFile;
use super::exec_start::ExecStart;

/// What one group of the property dump amounts to.
///
/// A named type rather than a tuple, because the dump carries three properties now and a
/// caller reading `shown.1` would say nothing about which. Both collectors that read the dump
/// take this, so a fourth property reaches them without changing either signature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShownUnit {
    /// What systemd will run, resolved through every drop-in, in the order it runs them.
    pub exec_start: Vec<ExecStart>,
    /// The variables the unit declares, keyed by name so a diff has one order.
    ///
    /// **This is what the unit file says, not what the process gets.** systemd reads an
    /// `EnvironmentFile=` at exec time rather than at load time, so what those files
    /// contribute is absent here — measured against systemd 257, where a variable set only
    /// in the file did not appear on this line. [`Self::environment_files`] is the other
    /// half, and neither field is the whole answer on its own.
    pub environment: BTreeMap<EnvironmentVariableName, String>,
    /// The files the unit reads its environment from, **in systemd's order**.
    ///
    /// A list and not a map, and never sorted: systemd reads these in the order the unit
    /// declares them and a later file overrides a variable an earlier one set, so the order
    /// *is* part of the meaning. Sorting them would be the same mistake as sorting
    /// `/proc/mounts`.
    pub environment_files: Vec<EnvironmentFile>,
}
