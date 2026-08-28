//! Reading `pg_db_role_setting` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the three columns it asks for, the empty
//! database or role that means the override applies to every one, and the `name=value`
//! element `unnest` produces per stored default.
//!
//! **`unnest(setconfig)` rather than the array itself.** `setconfig` is a `text[]`, and psql
//! would print it as one `{...}` literal a reader would then re-parse with the array's own
//! quoting rules on top of the CSV's. Unnesting asks the server to hand back one element per
//! row instead, so the only thing left to split is a single `name=value`.
//!
//! **One connection to any database sees every override.** `pg_db_role_setting` is a shared
//! catalog with no `REVOKE`, so this read is cluster-wide from wherever the collector already
//! connected, and needs no privilege of its own.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterRoleSettings, RoleSetting};
use crate::collectors::postgresql::value_objects::{
    DatabaseName, RoleName, SettingName, SettingValue,
};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 3;

/// What separates a stored default's name from its value.
const ASSIGNMENT: char = '=';

/// A result set psql printed, ready to be read as role and database overrides.
pub struct PsqlRoleSettings;

impl PsqlRoleSettings {
    /// Reads `datname,rolname,setconfig-element` rows into a cluster's overrides.
    pub fn parse(output: &str) -> Result<ClusterRoleSettings, CollectionError> {
        let mut settings = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            let (name, value) = assignment_of(&record[2])?;

            settings.push(RoleSetting {
                database: scope_database(&record[0])?,
                role: scope_role(&record[1])?,
                name,
                value,
            });
        }

        ClusterRoleSettings::new(settings)
    }
}

/// An empty database is every database.
///
/// `coalesce` in the query renders a null `setdatabase` (the `ALTER ROLE` form, with no
/// database) as an empty column, and psql renders a real null the same way; both mean the
/// override is not scoped to one database.
fn scope_database(column: &str) -> Result<Option<DatabaseName>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    DatabaseName::new(column).map(Some)
}

/// An empty role is every role.
///
/// The same `coalesce`, for the `ALTER DATABASE` form that names no role.
fn scope_role(column: &str) -> Result<Option<RoleName>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    RoleName::new(column).map(Some)
}

/// Splits a `setconfig` element into the setting it names and the value it assigns.
///
/// Split on the first `=` only: a value may contain more (`primary_conninfo` holds
/// `host=... password=...`), and only the first separates the name from the rest.
fn assignment_of(element: &str) -> Result<(SettingName, SettingValue), CollectionError> {
    let (name, value) = element.split_once(ASSIGNMENT).ok_or_else(|| {
        CollectionError::new(format!(
            "psql printed the stored default {element:?}, which carries no {ASSIGNMENT:?} to \
             separate the setting from its value"
        ))
    })?;

    Ok((SettingName::new(name)?, SettingValue::new(value)))
}
