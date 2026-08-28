//! Reading `pg_hba_file_rules` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the two shapes the view has across versions,
//! and an empty column that means the server left the field null (a `local` rule has no
//! address, a malformed line has almost nothing but an error).
//!
//! **Two shapes, told apart by column count.** `rule_number` and `file_name` are PostgreSQL
//! 16 additions, so the view is eleven columns on 16 and later and nine on 15. The parser
//! reads whichever it is handed rather than being told the version, so the caller's only job
//! is to ask for the columns the target actually has.
//!
//! **Superuser only.** The view is revoked from PUBLIC and `pg_read_all_settings` does not
//! lift it, so a non-superuser read fails loudly rather than returning a short set. That is
//! the same privilege the roles read already needs.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterHbaRules, HbaRule};

/// The nine columns PostgreSQL 15 prints.
const COLUMNS_V15: usize = 9;

/// The eleven columns PostgreSQL 16 and later print, `rule_number` and `file_name` ahead of
/// the rest.
const COLUMNS_V16: usize = 11;

/// A result set psql printed, ready to be read as authentication rules.
pub struct PsqlHbaRules;

impl PsqlHbaRules {
    /// Reads either shape of `pg_hba_file_rules` into a cluster's rules.
    pub fn parse(output: &str) -> Result<ClusterHbaRules, CollectionError> {
        let mut rules = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            let rule = match record.len() {
                COLUMNS_V16 => HbaRule {
                    rule_number: whole_number(&record[0])?,
                    file_name: present(&record[1]),
                    line_number: whole_number(&record[2])?,
                    connection_type: present(&record[3]),
                    databases: present(&record[4]),
                    users: present(&record[5]),
                    address: present(&record[6]),
                    netmask: present(&record[7]),
                    auth_method: present(&record[8]),
                    options: present(&record[9]),
                    error: present(&record[10]),
                },
                COLUMNS_V15 => HbaRule {
                    rule_number: None,
                    file_name: None,
                    line_number: whole_number(&record[0])?,
                    connection_type: present(&record[1]),
                    databases: present(&record[2]),
                    users: present(&record[3]),
                    address: present(&record[4]),
                    netmask: present(&record[5]),
                    auth_method: present(&record[6]),
                    options: present(&record[7]),
                    error: present(&record[8]),
                },
                other => {
                    return Err(CollectionError::new(format!(
                        "pg_hba_file_rules printed a row of {other} fields, where 9 (PostgreSQL \
                         15) or 11 (16 and later) are the shapes rastro reads"
                    )));
                }
            };

            rules.push(rule);
        }

        ClusterHbaRules::new(rules)
    }
}

/// An empty column is an absent number.
fn whole_number(column: &str) -> Result<Option<i64>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    column.parse::<i64>().map(Some).map_err(|_| {
        CollectionError::new(format!(
            "psql printed {column:?} where pg_hba_file_rules has a whole number"
        ))
    })
}

/// An empty column is an absent value.
fn present(column: &str) -> Option<String> {
    if column.is_empty() {
        return None;
    }

    Some(column.to_owned())
}
