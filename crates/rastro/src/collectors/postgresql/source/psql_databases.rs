//! Reading `pg_database` out of a psql result set.
//!
//! **The ACL is not read here.** Its contents come from `aclexplode` in
//! [`PsqlDatabaseGrants`](super::psql_database_grants::PsqlDatabaseGrants), because the text
//! form of an `aclitem` cannot be tokenised: the delimiters are legal inside the identifiers
//! they separate. What this query does read is whether there is an ACL at all, which no
//! rendering of its contents can answer.
//!
//! **Null and empty are different states and the text form cannot tell them apart.**
//! `array_to_string` gives an empty string for both a null array and an empty one, verified
//! on the reference cluster. Null means no grant was ever made or revoked, so the built-in
//! defaults apply and `PUBLIC` may connect; empty means everything has been revoked from
//! everybody. Conflating them would claim the defaults apply to a database that has had them
//! taken away, so the query asks `datacl IS NULL` outright.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterDatabases, Database};
use crate::collectors::postgresql::value_objects::{DatabaseName, RoleName};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 5;

/// What the server prints for a database that allows as many connections as it likes.
const NO_CONNECTION_LIMIT: &str = "-1";

/// A result set psql printed, ready to be read as databases.
pub struct PsqlDatabases;

impl PsqlDatabases {
    /// Reads `datname,owner,datallowconn,datconnlimit,acl_is_null` rows into a cluster's
    /// databases.
    ///
    /// A database whose ACL is null takes `None` for its grants; one that has an ACL starts
    /// with an empty list, which the grants read fills in. So a database that survives both
    /// reads with an empty list is one whose privileges have all been revoked.
    pub fn parse(output: &str) -> Result<ClusterDatabases, CollectionError> {
        let mut databases = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            let acl_is_default = PsqlResultSet::boolean(&record[4])?;

            databases.push(Database {
                name: DatabaseName::new(&record[0])?,
                owner: RoleName::new(&record[1])?,
                allows_connections: PsqlResultSet::boolean(&record[2])?,
                connection_limit: connection_limit_of(&record[3])?,
                grants: if acl_is_default {
                    None
                } else {
                    Some(Vec::new())
                },
                // Filled in by a second pass: `pg_extension` is per database, so it needs a
                // connection to that database rather than to the cluster.
                extensions: None,
            });
        }

        ClusterDatabases::new(databases)
    }
}

/// `-1` is no limit rather than a limit of minus one.
fn connection_limit_of(column: &str) -> Result<Option<i64>, CollectionError> {
    if column == NO_CONNECTION_LIMIT {
        return Ok(None);
    }

    column.parse::<i64>().map(Some).map_err(|_| {
        CollectionError::new(format!(
            "psql printed {column:?} where a connection limit is a whole number"
        ))
    })
}
