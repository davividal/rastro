//! One server setting, in rastro's terms.

use rastro_collector::Observation;

use crate::collectors::postgresql::value_objects::{
    SettingName, SettingSource, SettingUnit, SettingValue,
};

/// A setting as the running server reports it.
///
/// A plain aggregate: every field is already a validated value. It carries `source` and
/// `pending_restart` because both describe the host rather than the value, and both are
/// what a diff of two clusters is actually read for: whether somebody chose this, and
/// whether the running server has taken it up yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub name: SettingName,
    pub value: SettingValue,
    pub unit: Option<SettingUnit>,
    pub source: SettingSource,

    /// The configuration file asks for a value the running server has not adopted.
    ///
    /// Drift that no file comparison can see: `postgresql.conf` and the server disagree,
    /// and the file alone looks correct.
    pub pending_restart: bool,
}

impl From<&Setting> for Observation {
    fn from(setting: &Setting) -> Self {
        Observation::object([
            ("value", Observation::text(setting.value.as_str())),
            (
                "unit",
                match &setting.unit {
                    Some(unit) => Observation::text(unit.as_str()),
                    None => Observation::null(),
                },
            ),
            ("source", Observation::text(setting.source.as_str())),
            (
                "pending_restart",
                Observation::boolean(setting.pending_restart),
            ),
        ])
    }
}
