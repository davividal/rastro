//! Reading `pg_roles` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the nine columns it asks for, the `-1` that
//! means no connection limit, and the empty timestamp that means no expiry.
//!
//! **The hash is never selected, only a `CASE` over it.** The attributes come from
//! `pg_roles`, which masks the password; the tenth column is a derived name for how the
//! password is stored, and the join to `pg_authid` exists for that one expression. So the
//! secret stays on the server while the fact that there is one, and how it is kept, reaches
//! the document.
//!
//! **The `pg_` roles are left out by the query.** `pg_monitor`, `pg_read_all_stats` and the
//! rest arrive with the server version, which the document already records, so they are the
//! same on every cluster of that version and would be noise on all of them. The prefix is
//! reserved, so nothing an administrator creates can hide behind the filter. Membership *in*
//! one of them is a different matter and is a per-cluster fact worth having.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterRoles, Role};
use crate::collectors::postgresql::value_objects::{PasswordMethod, RoleName};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 10;

/// What the server prints for a role that may hold as many connections as it likes.
const NO_CONNECTION_LIMIT: &str = "-1";

/// A result set psql printed, ready to be read as roles.
pub struct PsqlRoles;

impl PsqlRoles {
    /// Reads `rolname,rolsuper,rolcreatedb,rolcreaterole,rolreplication,rolbypassrls,
    /// rolcanlogin,rolconnlimit,rolvaliduntil` rows into a cluster's roles.
    pub fn parse(output: &str) -> Result<ClusterRoles, CollectionError> {
        let mut roles = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            roles.push(Role {
                name: RoleName::new(&record[0])?,
                superuser: PsqlResultSet::boolean(&record[1])?,
                creates_databases: PsqlResultSet::boolean(&record[2])?,
                creates_roles: PsqlResultSet::boolean(&record[3])?,
                replication: PsqlResultSet::boolean(&record[4])?,
                bypasses_row_level_security: PsqlResultSet::boolean(&record[5])?,
                can_login: PsqlResultSet::boolean(&record[6])?,
                connection_limit: connection_limit_of(&record[7])?,
                valid_until: expiry_of(&record[8]),
                password_method: password_method_of(&record[9])?,
            });
        }

        ClusterRoles::new(roles)
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

/// An empty method is no password.
fn password_method_of(column: &str) -> Result<Option<PasswordMethod>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    Ok(Some(PasswordMethod::of(column)?))
}

/// An empty expiry is no expiry.
///
/// psql renders a null and an empty string identically, and for this column both mean the
/// password does not expire.
fn expiry_of(column: &str) -> Option<String> {
    if column.is_empty() {
        return None;
    }

    Some(column.to_owned())
}
