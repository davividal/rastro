//! One row of `systemctl list-unit-files --output=json`.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::units::model::UnitFile;
use crate::collectors::units::value_objects::{PresetState, UnitFileState, UnitName};

/// systemd's spelling of an installed unit, kept apart from rastro's meaning.
///
/// The field names are systemd's, which is why they are here and nowhere else:
/// `unit_file` rather than `name`, and a `preset` that is `null` far more often than
/// not.
#[derive(Debug, Clone, Deserialize)]
pub struct UnitFileRow {
    unit_file: String,
    state: String,
    /// `null` for the majority of units, and absent altogether on a systemd too old to
    /// report presets, which `serde(default)` covers.
    #[serde(default)]
    preset: Option<String>,
}

impl UnitFileRow {
    /// Translates systemd's row into rastro's model.
    pub fn to_unit_file(&self) -> Result<(UnitName, UnitFile), CollectionError> {
        let file = UnitFile {
            state: UnitFileState::new(self.state.clone())?,
            preset: optional_preset(self.preset.as_deref())?,
        };

        Ok((UnitName::new(self.unit_file.clone())?, file))
    }
}

/// A preset that systemd reported as absent, or as the empty string.
///
/// The empty case is folded into absence rather than refused. `--output=json` gives
/// `null`, but the table `--plain` prints writes `-`, and a systemd between the two
/// spellings could write neither; none of the three is a preset, and treating an empty
/// one as a failure would lose the whole facet over a formatting detail.
fn optional_preset(preset: Option<&str>) -> Result<Option<PresetState>, CollectionError> {
    match preset {
        None => Ok(None),
        Some(value) if value.is_empty() || value == "-" => Ok(None),
        Some(value) => Ok(Some(PresetState::new(value)?)),
    }
}
