//! Reading `pg_replication_slots` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the six stable columns it asks for, and an
//! empty plugin or database that means the slot is physical rather than logical.
//!
//! The volatile columns are never selected, so nothing here has to annotate them: a slot's
//! LSNs and its active flag move as it works, and only its identity and shape are read.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterReplicationSlots, ReplicationSlot};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 6;

/// A result set psql printed, ready to be read as replication slots.
pub struct PsqlReplicationSlots;

impl PsqlReplicationSlots {
    /// Reads `slot_name,plugin,slot_type,database,temporary,two_phase` rows into a cluster's
    /// slots.
    pub fn parse(output: &str) -> Result<ClusterReplicationSlots, CollectionError> {
        let mut slots = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            if record[0].is_empty() {
                return Err(CollectionError::new(
                    "pg_replication_slots reported a slot with no name",
                ));
            }

            slots.push(ReplicationSlot {
                name: record[0].clone(),
                plugin: present(&record[1]),
                slot_type: record[2].clone(),
                database: present(&record[3]),
                temporary: PsqlResultSet::boolean(&record[4])?,
                two_phase: PsqlResultSet::boolean(&record[5])?,
            });
        }

        ClusterReplicationSlots::new(slots)
    }
}

/// An empty column is an absent value: a physical slot has no plugin and no database.
fn present(column: &str) -> Option<String> {
    if column.is_empty() {
        return None;
    }

    Some(column.to_owned())
}
