//! Reading `pg_settings` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the columns it asks for, an empty unit that
//! means no unit, and the row that describes rastro's own connection rather than the host.
//! How psql spells a result set at all is
//! [`PsqlResultSet`](super::psql_result_set::PsqlResultSet).

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterSettings, Setting};
use crate::collectors::postgresql::value_objects::{
    SettingName, SettingSource, SettingUnit, SettingValue,
};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 8;

/// The source a server reports for a value that the connecting client set.
///
/// psql sets `application_name` on every connection it opens, so the server reports it
/// with this source. It describes rastro's own session, so recording it would put the
/// fingerprinting tool into the fingerprint and call it the state of the host.
const SET_BY_OUR_OWN_CONNECTION: &str = "client";

/// A result set psql printed, ready to be read as settings.
pub struct PsqlSettings;

impl PsqlSettings {
    /// Reads `name,setting,unit,source,context,pending_restart,sourcefile,sourceline` rows
    /// into a cluster's configuration.
    pub fn parse(output: &str) -> Result<ClusterSettings, CollectionError> {
        let mut settings = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            if record[3] == SET_BY_OUR_OWN_CONNECTION {
                continue;
            }

            settings.push(Setting {
                name: SettingName::new(&record[0])?,
                value: SettingValue::new(&record[1]),
                unit: unit_of(&record[2])?,
                source: SettingSource::new(&record[3])?,
                pending_restart: PsqlResultSet::boolean(&record[5])?,
                context: record[4].clone(),
                sourcefile: present(&record[6]),
                sourceline: source_line(&record[7])?,
            });
        }

        ClusterSettings::new(settings)
    }
}

/// An empty unit is no unit.
///
/// psql renders a null and an empty string identically, and for this column both mean the
/// same thing: the setting is not measured in anything.
fn unit_of(column: &str) -> Result<Option<SettingUnit>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    Ok(Some(SettingUnit::new(column)?))
}

/// An empty column is an absent value.
///
/// A setting from a default or the command line has no source file, and one read by an
/// unprivileged role has it nulled; psql renders both as an empty column.
fn present(column: &str) -> Option<String> {
    if column.is_empty() {
        return None;
    }

    Some(column.to_owned())
}

/// An empty source line is no line, on the same terms as the source file.
fn source_line(column: &str) -> Result<Option<i64>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    column.parse::<i64>().map(Some).map_err(|_| {
        CollectionError::new(format!(
            "psql printed {column:?} where pg_settings sourceline is a whole number"
        ))
    })
}
