//! Reading the cluster's control-file identity out of a psql result set.
//!
//! One row, two columns, from the `pg_control_*` functions: the system identifier and the
//! timeline. The whole family is EXECUTE-to-PUBLIC, so this needs no privilege. Only these
//! two columns are read; the rest are counters that move on every checkpoint.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::ControlData;

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 2;

/// A result set psql printed, ready to be read as the control-file identity.
pub struct PsqlControlData;

impl PsqlControlData {
    /// Reads `system_identifier,timeline_id` into the cluster's lineage.
    pub fn parse(output: &str) -> Result<ControlData, CollectionError> {
        let mut rows = PsqlResultSet::rows(output)?.into_iter();

        let row = rows.next().ok_or_else(|| {
            CollectionError::new(
                "pg_control returned no row, and a cluster always has a control file",
            )
        })?;
        PsqlResultSet::expect_columns(&row, COLUMNS)?;

        if rows.next().is_some() {
            return Err(CollectionError::new(
                "pg_control returned more than one row, which its control file never is",
            ));
        }

        let system_identifier = row[0].clone();
        if system_identifier.is_empty() {
            return Err(CollectionError::new(
                "pg_control_system() reported an empty system identifier",
            ));
        }

        let timeline_id = row[1].parse::<i64>().map_err(|_| {
            CollectionError::new(format!(
                "psql printed {:?} where pg_control_checkpoint()'s timeline is a whole number",
                row[1]
            ))
        })?;

        Ok(ControlData {
            system_identifier,
            timeline_id,
        })
    }
}
