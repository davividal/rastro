//! One unit, from both sides at once.

use rastro_collector::Observation;

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
}

impl From<&Unit> for Observation {
    fn from(unit: &Unit) -> Self {
        Observation::object([
            (
                "exec_start",
                Observation::list(unit.exec_start.iter().map(Observation::from)),
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
