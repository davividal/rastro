//! The role and database a cluster's settings were read through.

use rastro_collector::Observation;

use crate::collectors::postgresql::value_objects::{DatabaseName, RoleName};

/// Who rastro was, and where it was connected, when it read the settings.
///
/// `pg_settings` is one session's view, not the cluster's. It folds the connecting role's and
/// database's `ALTER ... SET` defaults into its map, and it silently drops the
/// `GUC_SUPERUSER_ONLY` rows for a role that is neither a superuser nor a member of
/// `pg_read_all_settings`. Neither distortion can be read back out of the settings
/// themselves, so the lens is recorded beside them: without it a reader cannot tell which of
/// the two applies to the map they are diffing, nor whether 21 settings are missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLens {
    pub role: RoleName,
    pub database: DatabaseName,
    pub is_superuser: bool,

    /// Whether the reading role holds `pg_read_all_settings`.
    ///
    /// The one grant that lifts the `GUC_SUPERUSER_ONLY` filter for a non-superuser, so it
    /// decides, together with `is_superuser`, whether the settings map is complete.
    pub reads_all_settings: bool,
}

impl ReadLens {
    /// Whether every setting was visible through this lens.
    ///
    /// False means the 21 `GUC_SUPERUSER_ONLY` parameters were dropped from the map with no
    /// word from the server, which is what the cluster records as `settings_complete`.
    pub fn sees_all_settings(&self) -> bool {
        self.is_superuser || self.reads_all_settings
    }
}

impl From<&ReadLens> for Observation {
    fn from(lens: &ReadLens) -> Self {
        Observation::object([
            ("role", Observation::text(lens.role.as_str())),
            ("database", Observation::text(lens.database.as_str())),
            ("is_superuser", Observation::boolean(lens.is_superuser)),
            (
                "reads_all_settings",
                Observation::boolean(lens.reads_all_settings),
            ),
        ])
    }
}
