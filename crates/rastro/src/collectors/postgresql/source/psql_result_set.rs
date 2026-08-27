//! The `psql --csv` interface, shared by every query the collector runs.
//!
//! psql's spelling of a result set, kept apart from rastro's meaning. Everything peculiar
//! to this interface lives here: RFC 4180 quoting, a null and an empty string that arrive
//! looking identical, and a boolean printed as one letter.
//!
//! **Why CSV and not `-A -F<separator>`.** A separator can appear in a value.
//! `archive_command` holds a shell command and `log_line_prefix` holds a format string, so
//! any character picked as a separator is one an operator is allowed to use. CSV is the one
//! output psql offers that quotes rather than hopes.

use rastro_collector::CollectionError;

/// A result set psql printed.
pub struct PsqlResultSet;

impl PsqlResultSet {
    /// Splits CSV into records of fields.
    ///
    /// A state machine over the whole text rather than a split per line, because a quoted
    /// field may contain the record separator: `archive_command` is a shell command and
    /// nothing stops it spanning lines. Reading line by line would turn one such value into
    /// two malformed rows.
    ///
    /// A bare `\r` is dropped outside quotes so CRLF output reads the same, and kept inside
    /// them, where it is content.
    pub fn rows(csv: &str) -> Result<Vec<Vec<String>>, CollectionError> {
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

    /// Checks a row has the columns the query asked for.
    ///
    /// Refused rather than padded: a short row means the query and the parser disagree
    /// about the columns, and guessing which one is missing puts a value under the wrong
    /// name.
    pub fn expect_columns(record: &[String], columns: usize) -> Result<(), CollectionError> {
        if record.len() == columns {
            return Ok(());
        }

        Err(CollectionError::new(format!(
            "psql printed a row of {} values where the query asks for {columns}, so which \
             column is missing cannot be told: {record:?}",
            record.len()
        )))
    }

    /// Reads a boolean the way postgres prints one.
    pub fn boolean(column: &str) -> Result<bool, CollectionError> {
        match column {
            "t" => Ok(true),
            "f" => Ok(false),
            other => Err(CollectionError::new(format!(
                "psql printed {other:?} where a boolean is either \"t\" or \"f\""
            ))),
        }
    }
}
