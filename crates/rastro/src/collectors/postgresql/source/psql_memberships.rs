//! Reading `pg_auth_members` out of a psql result set.
//!
//! **Only `admin_option`, and that is a version decision.** PostgreSQL 16 added
//! `inherit_option` and `set_option` to this catalogue. Selecting them would make the query
//! fail outright on a 15 or a 12, and this collector reads whatever a Debian box has
//! side by side after an upgrade. `admin_option` has been there throughout, so one query
//! answers on every version rather than the collector having to know which it is talking to.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{ClusterMemberships, Membership};
use crate::collectors::postgresql::value_objects::RoleName;

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 3;

/// A result set psql printed, ready to be read as memberships.
pub struct PsqlMemberships;

impl PsqlMemberships {
    /// Reads `member,granted,admin_option` rows into a cluster's memberships.
    pub fn parse(output: &str) -> Result<ClusterMemberships, CollectionError> {
        let mut memberships = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            memberships.push(Membership {
                member: RoleName::new(&record[0])?,
                granted: RoleName::new(&record[1])?,
                admin_option: PsqlResultSet::boolean(&record[2])?,
            });
        }

        ClusterMemberships::new(memberships)
    }
}
