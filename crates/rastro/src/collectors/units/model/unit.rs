//! One unit, from both sides at once.

use rastro_collector::Observation;

use super::unit_file::UnitFile;
use super::unit_runtime::UnitRuntime;

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
}

impl From<&Unit> for Observation {
    fn from(unit: &Unit) -> Self {
        Observation::object([
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
