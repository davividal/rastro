//! One unit, from both sides at once.

use std::collections::BTreeMap;

use rastro_collector::{EnvironmentVariableName, Observation};

use super::environment_source::EnvironmentSource;
use super::unit_file::UnitFile;
use super::unit_runtime::UnitRuntime;
use crate::collectors::systemd::ExecStart;

/// A unit as rastro means it: whatever is on disk, and whatever systemd has loaded.
///
/// **Both sides are optional, and each absence says something different.** This is the
/// type that makes the outer join legible, and the numbers from the development box are
/// the argument for doing the join at all: of 333 distinct names, 156 have both sides,
/// 106 have only a file and 71 have only a loaded unit.
///
/// - **A file with no loaded unit** is installed and never referenced. Most are
///   templates such as `autovt@.service`, which are instantiated rather than loaded,
///   and targets nothing pulls in.
/// - **A loaded unit with no file** is either something systemd created itself, such as
///   the `-.slice` root slice or an instantiated `blockdev@...target`, or a dangling
///   reference: `NetworkManager.service` is loaded `not-found` on a box that has never
///   had NetworkManager installed.
///
/// Reporting only one side would hide one of those two populations entirely, and both
/// are things an operator wants a diff to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub file: Option<UnitFile>,
    pub runtime: Option<UnitRuntime>,
    /// What systemd will actually run, resolved through every drop-in, in the order it
    /// runs them.
    ///
    /// **This is the field that makes the facet answer "what is running on this box".**
    /// The state either side of it says a unit is enabled and active; only this says which
    /// binary that amounts to and which flags it was given, which is where a change to a
    /// deployment actually shows up.
    ///
    /// Empty for the many units that start nothing — targets, slices, most sockets — and
    /// for a unit file systemd has not loaded, since an unresolved file is not a claim
    /// rastro can make about what would run.
    pub exec_start: Vec<ExecStart>,
    /// What the unit sets for the process it starts, resolved through every drop-in.
    ///
    /// **Names are recorded in the clear and values are marked sensitive.** A name says
    /// which variable a service depends on, which is the thing an operator moving a box
    /// needs to know and is not itself a secret; a value is where the database password
    /// actually lives on most boxes that have one. Redaction still diffs, so a rotated
    /// credential shows as a changed digest without the document carrying it.
    ///
    /// **Empty carries three different meanings, and only the first is "sets nothing".**
    /// Measured on a box running real systemd rather than reasoned about:
    ///
    /// - The unit declares no variables.
    /// - The unit's environment comes from an `EnvironmentFile=`, which systemd opens at
    ///   exec time and which therefore contributes nothing to this field.
    ///   [`Self::environment_files`] is where that unit shows up.
    /// - **systemd has not loaded the unit**, so it was never asked. The `show` request
    ///   names the units `list-units` returned, for the reason `exec_start` documents: an
    ///   unresolved unit file has no effective configuration to report, and systemd
    ///   garbage-collects a static unit that is inactive and unreferenced, so a unit file
    ///   on disk is not evidence that this field was ever populated.
    ///
    /// Only the first two are answerable from this facet alone. The third is visible
    /// because [`Self::runtime`] is `null` for a unit systemd has not loaded.
    pub environment: BTreeMap<EnvironmentVariableName, String>,
    /// The files the unit reads its environment from, in the order systemd reads them,
    /// each with what rastro found in it.
    ///
    /// Never sorted: systemd reads these in the order the unit declares them and a later
    /// file overrides a variable an earlier one set, so the order *is* part of the meaning.
    /// The same reasoning that keeps kernel order in `/proc/mounts`.
    pub environment_files: Vec<EnvironmentSource>,
    /// The names systemd removes once everything above has been assembled.
    ///
    /// **A variable named here does not reach the process, whatever set it.** Without this
    /// field the facet would report a service as having a variable it never receives, which
    /// is the one thing a fingerprint must not do.
    pub unset_environment: Vec<EnvironmentVariableName>,
}

impl From<&Unit> for Observation {
    fn from(unit: &Unit) -> Self {
        Observation::object([
            (
                "environment",
                Observation::object(
                    unit.environment
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value).sensitive())),
                ),
            ),
            (
                "environment_files",
                Observation::list(unit.environment_files.iter().map(Observation::from)),
            ),
            (
                "exec_start",
                Observation::list(unit.exec_start.iter().map(Observation::from)),
            ),
            (
                "unset_environment",
                Observation::list(
                    unit.unset_environment
                        .iter()
                        .map(|name| Observation::text(name.as_str())),
                ),
            ),
            (
                "file",
                unit.file
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "runtime",
                unit.runtime
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
        ])
    }
}
