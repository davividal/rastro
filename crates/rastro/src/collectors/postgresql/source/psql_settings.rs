//! The `psql --csv` interface.
//!
//! psql's spelling of a result set, kept apart from rastro's meaning. Everything peculiar
//! to this interface lives here: RFC 4180 quoting, a null and an empty string that arrive
//! looking identical, and the row that describes rastro's own connection rather than the
//! host.
//!
//! **Why CSV and not `-A -F<separator>`.** A separator can appear in a value.
//! `archive_command` holds a shell command and `log_line_prefix` holds a format string, so
//! any character picked as a separator is one an operator is allowed to use. CSV is the one
//! output psql offers that quotes rather than hopes.

use rastro_collector::CollectionError;

use crate::collectors::postgresql::model::{ClusterSettings, Setting};
use crate::collectors::postgresql::value_objects::{
    SettingName, SettingSource, SettingUnit, SettingValue,
};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 6;

/// The source a server reports for a value that the connecting client set.
///
/// psql sets `application_name` on every connection it opens, so the server reports it
/// with this source. It describes rastro's own session, so recording it would put the
/// fingerprinting tool into the fingerprint and call it the state of the host.
const SET_BY_OUR_OWN_CONNECTION: &str = "client";

/// A result set psql printed, ready to be read as settings.
pub struct PsqlSettings;

impl PsqlSettings {
    /// Reads `name,setting,unit,source,context,pending_restart` rows into a cluster's
    /// configuration.
    pub fn parse(output: &str) -> Result<ClusterSettings, CollectionError> {
        let mut settings = Vec::new();

        for record in records(output)? {
            if record.len() != COLUMNS {
                return Err(CollectionError::new(format!(
                    "psql printed a row of {} values where the query asks for {COLUMNS}, so \
                     which column is missing cannot be told: {record:?}",
                    record.len()
                )));
            }

            if record[3] == SET_BY_OUR_OWN_CONNECTION {
                continue;
            }

            settings.push(Setting {
                name: SettingName::new(&record[0])?,
                value: SettingValue::new(&record[1]),
                unit: unit_of(&record[2])?,
                source: SettingSource::new(&record[3])?,
                pending_restart: pending_restart_of(&record[5])?,
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

/// Postgres prints a boolean as `t` or `f`.
fn pending_restart_of(column: &str) -> Result<bool, CollectionError> {
    match column {
        "t" => Ok(true),
        "f" => Ok(false),
        other => Err(CollectionError::new(format!(
            "psql printed {other:?} where a boolean is either \"t\" or \"f\""
        ))),
    }
}

/// Splits CSV into records of fields.
///
/// A state machine over the whole text rather than a split per line, because a quoted
/// field may contain the record separator: `archive_command` is a shell command and
/// nothing stops it spanning lines. Reading line by line would turn one such setting into
/// two malformed rows.
///
/// A bare `\r` is dropped outside quotes so CRLF output reads the same, and kept inside
/// them, where it is content.
fn records(csv: &str) -> Result<Vec<Vec<String>>, CollectionError> {
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = csv.chars().peekable();

    while let Some(character) = characters.next() {
        if quoted {
            match character {
                // A doubled quote is one quote, which is how `search_path` arrives.
                '"' if characters.peek() == Some(&'"') => {
                    characters.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(character),
            }
            continue;
        }

        match character {
            // Only a quote opening a field quotes it; one inside an unquoted field is
            // content, which is what psql itself assumes when it decides not to quote.
            '"' if field.is_empty() => quoted = true,
            ',' => fields.push(std::mem::take(&mut field)),
            '\n' => {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
            }
            '\r' => {}
            _ => field.push(character),
        }
    }

    if quoted {
        return Err(CollectionError::new(
            "psql printed a quoted value that never ends, so the output is truncated",
        ));
    }

    // A last row that arrived without its newline.
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    // A blank line carries no values, and reporting it as a short row would blame the
    // query for something psql's own formatting does.
    records.retain(|record| record.iter().any(|field| !field.is_empty()));

    Ok(records)
}
