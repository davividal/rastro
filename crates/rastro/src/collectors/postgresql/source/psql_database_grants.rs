//! Reading database ACLs out of `aclexplode`, one row per privilege.
//!
//! **Rows rather than the text form of an `aclitem`, and that is a correctness decision
//! rather than a preference.** A role name may contain any character: on the reference box a
//! role called `reporting team=x` appears in a space-joined `datacl` as
//! `""reporting team=x""=C/postgres`, so splitting on whitespace and on the first `=` yields
//! `""reporting` as the grantee. There is no tokenising that is right in general, because the
//! delimiters are legal inside the identifiers they separate. `aclexplode` hands back
//! grantee, privilege, grant option and grantor as columns, and psql's CSV quoting then
//! carries a name with a comma or a quote in it unharmed.
//!
//! The server spells `PUBLIC` as grantee zero, which the query renders as an empty column.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{DatabaseGrants, Grant};
use crate::collectors::postgresql::value_objects::{
    DatabaseName, DatabasePrivilege, Grantee, RoleName,
};

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 5;

/// A result set psql printed, ready to be read as grants.
pub struct PsqlDatabaseGrants;

impl PsqlDatabaseGrants {
    /// Reads `datname,grantee,privilege,grantable,grantor` rows into the grants of each
    /// database.
    ///
    /// One row is one privilege, so rows are gathered per database, grantee and grantor: a
    /// reader wants what one grantee holds in one place, and the grant option belongs to the
    /// privilege rather than to the grantee.
    pub fn parse(output: &str) -> Result<DatabaseGrants, CollectionError> {
        let mut gathered: BTreeMap<
            (DatabaseName, Grantee, RoleName),
            BTreeMap<DatabasePrivilege, bool>,
        > = BTreeMap::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            let database = DatabaseName::new(&record[0])?;
            let grantee = grantee_of(&record[1])?;
            let privilege = DatabasePrivilege::of(&record[2])?;
            let grantable = PsqlResultSet::boolean(&record[3])?;
            let grantor = RoleName::new(&record[4])?;

            let privileges = gathered
                .entry((database, grantee.clone(), grantor.clone()))
                .or_default();

            if privileges.insert(privilege, grantable).is_some() {
                return Err(CollectionError::new(format!(
                    "the server granted {} on one database to {:?} twice, so whether it may be \
                     passed on cannot be told",
                    privilege.as_str(),
                    grantee.as_str()
                )));
            }
        }

        Ok(DatabaseGrants::new(gathered.into_iter().map(
            |((database, grantee, granted_by), privileges)| {
                (
                    database,
                    Grant {
                        grantee,
                        granted_by,
                        privileges,
                    },
                )
            },
        )))
    }
}

/// An empty grantee is grantee zero, which is `PUBLIC`.
fn grantee_of(column: &str) -> Result<Grantee, CollectionError> {
    if column.is_empty() {
        return Ok(Grantee::Public);
    }

    Ok(Grantee::Role(RoleName::new(column)?))
}
