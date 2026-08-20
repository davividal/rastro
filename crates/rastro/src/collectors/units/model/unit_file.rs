//! What is on disk for a unit.

use rastro_collector::Observation;

use crate::collectors::units::value_objects::{PresetState, UnitFileState};

/// The installed side of a unit: its enablement, and what the distribution intended.
///
/// A plain aggregate of two already-validated values. It exists as a type rather than
/// two fields on [`Unit`](super::Unit) so that "there is a unit file" and "there is a
/// loaded unit" are two `Option`s a reader cannot confuse, which is the whole point of
/// the join above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitFile {
    pub state: UnitFileState,
    /// Absent for a unit no preset policy covers, which is most of them.
    pub preset: Option<PresetState>,
}

impl From<&UnitFile> for Observation {
    fn from(file: &UnitFile) -> Self {
        Observation::object([
            (
                "preset",
                file.preset
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("state", Observation::from(&file.state)),
        ])
    }
}
