//! Every role or database default a cluster carries.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::RoleSetting;

/// The `pg_db_role_setting` overrides of one cluster, in a defined order.
///
/// **Empty is a real and common state**, which is why this type does not refuse an empty
/// vector where [`ClusterSettings`](super::ClusterSettings) does: most clusters carry no
/// `ALTER ROLE`/`ALTER DATABASE` default at all, so nothing here means "none set" rather than
/// a failed read.
///
/// Sorted by scope then name here rather than left to the server, because list order is part
/// of the output contract and an `ORDER BY` in a query is a promise made somewhere this type
/// cannot see. The same (database, role, name) triple appearing twice is refused: `setconfig`
/// holds a name once per scope, so a repeat means two reads were spliced or the catalog was
/// misread, and keeping whichever value rendered last would report one the cluster is not
/// applying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRoleSettings {
    settings: Vec<RoleSetting>,
}

impl ClusterRoleSettings {
    pub fn new(mut settings: Vec<RoleSetting>) -> Result<Self, CollectionError> {
        settings.sort();

        if let Some(pair) = settings.windows(2).find(|pair| {
            pair[0].database == pair[1].database
                && pair[0].role == pair[1].role
                && pair[0].name == pair[1].name
        }) {
            return Err(CollectionError::new(format!(
                "the server reported the override {:?} twice for one role and database, so which \
                 value it applies cannot be told",
                pair[0].name.as_str()
            )));
        }

        Ok(Self { settings })
    }

    pub fn settings(&self) -> &[RoleSetting] {
        &self.settings
    }
}

impl From<&ClusterRoleSettings> for Observation {
    fn from(settings: &ClusterRoleSettings) -> Self {
        Observation::list(
            settings
                .settings()
                .iter()
                .map(|setting| Observation::from(setting)),
        )
    }
}
