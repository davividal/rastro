//! One default a cluster carries for a role, a database, or the two together.

use rastro_collector::Observation;

use super::setting::value_observation;
use crate::collectors::postgresql::value_objects::{
    DatabaseName, RoleName, SettingName, SettingValue,
};

/// A setting default stored in `pg_db_role_setting`, in rastro's terms.
///
/// What `ALTER ROLE ... SET`, `ALTER DATABASE ... SET` and `ALTER ROLE ... IN DATABASE ...
/// SET` leave behind: a value the server applies at session start for a role, a database, or
/// a pairing of the two. It is the archetype of what this collector exists for, because it
/// survives reboots, appears in no file, and is invisible to a `pg_settings` read as any
/// other role. `pg_settings` folds whichever of these matches the connecting session into
/// its own map and reports it as cluster configuration, so recording the overrides apart is
/// what lets a diff tell a scoped value from a global one.
///
/// **Scope is two optional fields, because the catalog keys on two oids.** An absent database
/// is `ALTER ROLE` for every database; an absent role is `ALTER DATABASE` for every role;
/// both absent is `ALTER ROLE ALL`, the default for every role in every database.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoleSetting {
    pub database: Option<DatabaseName>,
    pub role: Option<RoleName>,
    pub name: SettingName,
    pub value: SettingValue,
}

impl From<&RoleSetting> for Observation {
    fn from(setting: &RoleSetting) -> Self {
        Observation::object([
            (
                "database",
                match &setting.database {
                    Some(database) => Observation::text(database.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "role",
                match &setting.role {
                    Some(role) => Observation::text(role.as_str()),
                    None => Observation::null(),
                },
            ),
            ("name", Observation::text(setting.name.as_str())),
            ("value", value_observation(&setting.name, &setting.value)),
        ])
    }
}
