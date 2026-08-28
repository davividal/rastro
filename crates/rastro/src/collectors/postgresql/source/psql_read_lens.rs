//! Reading the connection's own identity out of a psql result set.
//!
//! One row, four columns, describing the session `pg_settings` was read through rather than
//! the host: the role, the database, whether the role is a superuser, and whether it holds
//! `pg_read_all_settings`. A live connection always has an identity, so no row, or more than
//! one, is a failure rather than an absence.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::ReadLens;
use crate::collectors::postgresql::value_objects::{DatabaseName, RoleName};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 4;

/// A result set psql printed, ready to be read as the lens.
pub struct PsqlReadLens;

impl PsqlReadLens {
    /// Reads `current_user,current_database,is_superuser,pg_read_all_settings` into the lens.
    pub fn parse(output: &str) -> Result<ReadLens, CollectionError> {
        let mut rows = PsqlResultSet::rows(output)?.into_iter();

        let row = rows.next().ok_or_else(|| {
            CollectionError::new(
                "the lens query returned no row, and a live connection always has an identity",
            )
        })?;
        PsqlResultSet::expect_columns(&row, COLUMNS)?;

        if rows.next().is_some() {
            return Err(CollectionError::new(
                "the lens query returned more than one row, which a session's identity never is",
            ));
        }

        Ok(ReadLens {
            role: RoleName::new(&row[0])?,
            database: DatabaseName::new(&row[1])?,
            is_superuser: PsqlResultSet::boolean(&row[2])?,
            reads_all_settings: PsqlResultSet::boolean(&row[3])?,
        })
    }
}
