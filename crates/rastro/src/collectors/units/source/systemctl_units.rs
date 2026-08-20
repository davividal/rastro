//! One row of `systemctl list-units --all --output=json`.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::systemd::UnitName;
use crate::collectors::units::model::UnitRuntime;
use crate::collectors::units::value_objects::{ActiveState, Description, LoadState, SubState};

/// systemd's spelling of a loaded unit, kept apart from rastro's meaning.
#[derive(Debug, Clone, Deserialize)]
pub struct UnitRow {
    unit: String,
    load: String,
    active: String,
    sub: String,
    /// **Not empty for a `not-found` unit, which is what a first guess expects.**
    /// systemd puts the unit's own name there instead: `NetworkManager.service` is
    /// reported with the description `"NetworkManager.service"`. That was measured on
    /// the development box, where 23 units are `not-found` and every one of them
    /// describes itself that way.
    ///
    /// So the emptiness handling below is defensive rather than a path this host takes,
    /// and `serde(default)` covers a systemd that omits the field altogether.
    #[serde(default)]
    description: String,
}

impl UnitRow {
    /// Translates systemd's row into rastro's model.
    pub fn to_runtime(&self) -> Result<(UnitName, UnitRuntime), CollectionError> {
        let runtime = UnitRuntime {
            load: LoadState::new(self.load.clone())?,
            active: ActiveState::new(self.active.clone())?,
            sub: SubState::new(self.sub.clone())?,
            description: optional_description(&self.description)?,
        };

        Ok((UnitName::new(self.unit.clone())?, runtime))
    }
}

/// A description systemd left blank, recorded as absent rather than as empty text.
///
/// Absent says systemd reported nothing; an empty string would claim a unit file
/// described itself as nothing. Note that this is *not* how a `not-found` unit arrives:
/// systemd substitutes the unit's own name for those, so they get a description like any
/// other unit. This exists for a systemd that reports the field empty or not at all.
fn optional_description(description: &str) -> Result<Option<Description>, CollectionError> {
    if description.is_empty() {
        return Ok(None);
    }

    Ok(Some(Description::new(description)?))
}
