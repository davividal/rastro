//! Reading `pg_file_settings` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the seven columns it asks for, an empty
//! source file or line that means the value did not come from a file, an empty error that
//! means the line applied, and an empty name that means PostgreSQL could not parse the line at
//! all, its `error` saying why.
//!
//! **Superuser only.** The view is revoked from PUBLIC and `pg_read_all_settings` does not
//! lift it, so a non-superuser read fails loudly (`permission denied for view
//! pg_file_settings`, 42501) rather than returning a short set. That failure is the right
//! answer, and it is the same privilege the roles read already needs, so it never narrows a
//! cluster that was otherwise readable.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterFileSettings, FileSetting};
use crate::collectors::postgresql::value_objects::{SettingName, SettingValue};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 7;

/// A result set psql printed, ready to be read as configuration-file lines.
pub struct PsqlFileSettings;

impl PsqlFileSettings {
    /// Reads `seqno,sourcefile,sourceline,name,setting,applied,error` rows into a cluster's
    /// file settings.
    pub fn parse(output: &str) -> Result<ClusterFileSettings, CollectionError> {
        let mut settings = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            settings.push(FileSetting {
                seqno: whole_number(&record[0], "seqno")?,
                sourcefile: present(&record[1]),
                sourceline: source_line(&record[2])?,
                name: setting_name(&record[3])?,
                value: SettingValue::new(&record[4]),
                applied: PsqlResultSet::boolean(&record[5])?,
                error: present(&record[6]),
            });
        }

        ClusterFileSettings::new(settings)
    }
}

/// A whole number, or a failure naming the column that was not one.
fn whole_number(column: &str, field: &str) -> Result<i64, CollectionError> {
    column.parse::<i64>().map_err(|_| {
        CollectionError::new(format!(
            "psql printed {column:?} where pg_file_settings {field} is a whole number"
        ))
    })
}

/// An empty source line is no line.
///
/// A value not read from a file (a command-line override, a default) carries no source line,
/// and psql renders that null and an empty string alike.
fn source_line(column: &str) -> Result<Option<i64>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    whole_number(column, "sourceline").map(Some)
}

/// An empty column is an absent value.
fn present(column: &str) -> Option<String> {
    if column.is_empty() {
        return None;
    }

    Some(column.to_owned())
}

/// The parameter name, absent on a line PostgreSQL could not parse.
///
/// A syntax error or an invalid parameter name leaves `pg_file_settings.name` null while the
/// `error` column explains it. Refusing the row would fail the whole read for the one line
/// this catalogue exists to surface, so an empty name is recorded as absent, not rejected.
fn setting_name(column: &str) -> Result<Option<SettingName>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    SettingName::new(column).map(Some)
}
