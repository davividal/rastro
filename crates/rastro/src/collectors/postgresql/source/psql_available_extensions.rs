//! Reading `pg_available_extensions` out of a psql result set.
//!
//! What is peculiar to *this query* lives here: the three columns it asks for, and an empty
//! installed version that means the extension is available but not created in the database
//! that answered.
//!
//! Public and cheap: `pg_available_extensions` reads the extension control files from disk,
//! so it needs no privilege and is the same in every database of the cluster, bar the
//! installed-version column, which belongs to the database that answered.

use rastro_collector::CollectionError;

use super::psql_result_set::PsqlResultSet;
use crate::collectors::postgresql::model::{AvailableExtension, ClusterAvailableExtensions};
use crate::collectors::postgresql::value_objects::ExtensionName;

/// The columns the collector's query asks for, in order.
const COLUMNS: usize = 3;

/// A result set psql printed, ready to be read as available extensions.
pub struct PsqlAvailableExtensions;

impl PsqlAvailableExtensions {
    /// Reads `name,default_version,installed_version` rows into a cluster's available
    /// extensions.
    pub fn parse(output: &str) -> Result<ClusterAvailableExtensions, CollectionError> {
        let mut extensions = Vec::new();

        for record in PsqlResultSet::rows(output)? {
            PsqlResultSet::expect_columns(&record, COLUMNS)?;

            extensions.push(AvailableExtension {
                name: ExtensionName::new(&record[0])?,
                default_version: record[1].clone(),
                installed_version: installed(&record[2]),
            });
        }

        ClusterAvailableExtensions::new(extensions)
    }
}

/// An empty installed version means the extension is available but not created here.
fn installed(column: &str) -> Option<String> {
    if column.is_empty() {
        return None;
    }

    Some(column.to_owned())
}
