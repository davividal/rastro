//! Reading `pg_replication_slots` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the stable columns it asks for, and an empty
//! plugin or database that means the slot is physical rather than logical.
//!
//! **Two shapes, told apart by column count.** `two_phase` is a PostgreSQL 14 addition, so a
//! 13 or earlier cluster answers five columns rather than six; there, a slot decodes no
//! two-phase commits, so the field defaults to false. The volatile columns are never
//! selected, so nothing here has to annotate a slot's moving LSNs or its active flag.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterReplicationSlots, ReplicationSlot};

/// The columns PostgreSQL 14 and later print.
const COLUMNS_WITH_TWO_PHASE: usize = 6;

/// The columns PostgreSQL 13 and earlier print, without `two_phase`.
const COLUMNS_WITHOUT_TWO_PHASE: usize = 5;

/// A result set psql printed, ready to be read as replication slots.
pub struct PsqlReplicationSlots;

impl PsqlReplicationSlots {
    /// Reads `slot_name,plugin,slot_type,database,temporary,two_phase` rows into a cluster's
    /// slots.
    pub fn parse(output: &str) -> Result<ClusterReplicationSlots, CollectionError> {
        let mut slots = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            let two_phase = match record.len() {
                COLUMNS_WITH_TWO_PHASE => PsqlResultSet::boolean(&record[5])?,
                COLUMNS_WITHOUT_TWO_PHASE => false,
                other => {
                    return Err(CollectionError::new(format!(
                        "pg_replication_slots printed a row of {other} fields, where 5 \
                         (PostgreSQL 13) or 6 (14 and later) are the shapes rastro reads"
                    )));
                }
            };

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
                two_phase,
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
