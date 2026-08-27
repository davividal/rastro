//! The grants of every database in a cluster, before they are joined to the databases.

use std::collections::BTreeMap;

use crate::collectors::postgresql::model::Grant;
use crate::collectors::postgresql::value_objects::DatabaseName;

/// Grants gathered per database.
///
/// A step between two reads rather than part of the document: `pg_database` says which
/// databases exist and whether each has an ACL at all, and `aclexplode` says what is in
/// those ACLs. Joining them is what produces a database's grants, and a database with a null
/// ACL takes none of this.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatabaseGrants {
    grants: BTreeMap<DatabaseName, Vec<Grant>>,
}

impl DatabaseGrants {
    pub fn new(grants: impl IntoIterator<Item = (DatabaseName, Grant)>) -> Self {
        let mut gathered: BTreeMap<DatabaseName, Vec<Grant>> = BTreeMap::new();

        for (database, grant) in grants {
            gathered.entry(database).or_default().push(grant);
        }

        // `PUBLIC` first and then by name, because `Grantee` orders that way and every
        // login role's `CONNECT` rests on the `PUBLIC` grant.
        for grants in gathered.values_mut() {
            grants.sort_by(|left, right| {
                left.grantee
                    .cmp(&right.grantee)
                    .then_with(|| left.granted_by.cmp(&right.granted_by))
            });
        }

        Self { grants: gathered }
    }

    /// The grants on one database, or `None` where the server reported none for it.
    pub fn of_database(&self, database: &str) -> Option<&[Grant]> {
        self.grants
            .iter()
            .find(|(name, _)| name.as_str() == database)
            .map(|(_, grants)| grants.as_slice())
    }

    /// Every database the grants mention, so a name the database list does not have can be
    /// reported rather than dropped.
    pub fn databases(&self) -> impl Iterator<Item = &DatabaseName> {
        self.grants.keys()
    }
}
