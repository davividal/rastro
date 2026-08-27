//! Every database in a cluster.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::Database;

/// The databases of one cluster, ordered by name.
///
/// Holds the two invariants a single database cannot: each name appears once, and there is
/// at least one. `template1` cannot be dropped, so a cluster with no databases is a failed
/// read rather than an empty cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterDatabases {
    databases: Vec<Database>,
}

impl ClusterDatabases {
    pub fn new(mut databases: Vec<Database>) -> Result<Self, CollectionError> {
        if databases.is_empty() {
            return Err(CollectionError::new(
                "the server reported no databases at all, and template1 cannot be dropped",
            ));
        }

        databases.sort_by(|left, right| left.name.cmp(&right.name));

        if let Some(repeated) = databases
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.as_str())
        {
            return Err(CollectionError::new(format!(
                "the server reported the database {repeated:?} twice, so who owns it cannot be \
                 told"
            )));
        }

        Ok(Self { databases })
    }

    pub fn databases(&self) -> &[Database] {
        &self.databases
    }
}

impl From<&ClusterDatabases> for Observation {
    fn from(databases: &ClusterDatabases) -> Self {
        Observation::object(
            databases
                .databases()
                .iter()
                .map(|database| (database.name.as_str(), Observation::from(database))),
        )
    }
}
