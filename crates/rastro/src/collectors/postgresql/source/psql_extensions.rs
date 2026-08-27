//! Reading `pg_extension` out of a psql result set.
//!
//! **Per database, which is why this is its own query.** `pg_extension` is not a shared
//! catalogue: what is installed is a property of the database, so this is the one read that
//! needs a connection per database rather than one per cluster. On a box with eleven
//! connectable databases that is eleven more psql invocations, which is the cost of the
//! answer being true rather than being whatever the database rastro happened to connect to
//! had installed.

use rastro_collector::{CollectionError, NonEmptyText};

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{DatabaseExtensions, Extension};
use crate::collectors::postgresql::value_objects::ExtensionName;

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 3;

/// A result set psql printed, ready to be read as extensions.
pub struct PsqlExtensions;

impl PsqlExtensions {
    /// Reads `extname,extversion,schema` rows into one database's extensions.
    pub fn parse(output: &str) -> Result<DatabaseExtensions, CollectionError> {
        let mut extensions = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            extensions.push(Extension {
                name: ExtensionName::new(&record[0])?,
                version: NonEmptyText::new(&record[1], "extension version")?,
                schema: NonEmptyText::new(&record[2], "extension schema")?,
            });
        }

        DatabaseExtensions::new(extensions)
    }
}
