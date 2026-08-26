//! The `pg_lsclusters` interface.
//!
//! postgresql-common's own register of what exists on the box, and the only place that
//! answers all four questions the facet needs at once: which clusters there are, whether
//! each is running, which port it is on, and **which account owns it**.
//!
//! **Why not enumerate `/etc/postgresql/*/*/`.** The directories are there for a cluster
//! that postgresql-common has dropped, and they carry no status and no owner. The owner in
//! particular cannot be guessed: `postgres` is the Debian default, not a rule, and the
//! whole point of reading a cluster as its owner is that peer authentication refuses
//! anybody else. Guessing it wrong turns a readable cluster into a reported failure.
//!
//! **Why not `pg_lsclusters --json`.** Newer postgresql-common offers it, Debian 12's does
//! not, and a collector that needs a flag the target distribution lacks reports failure on
//! the host it was written for. The text form has been stable across every release that
//! matters here.

use rastro_collector::CollectionError;

use crate::collectors::postgresql::value_objects::{ClusterId, ClusterStatus};

/// The fields `pg_lsclusters` prints, in order.
///
/// Seven columns, and the last two are paths. Only the first six are read, so a data
/// directory containing a space cannot shift a field that matters.
const MINIMUM_COLUMNS: usize = 6;

/// The header postgresql-common prints before the rows.
const HEADER_FIRST_COLUMN: &str = "Ver";

/// One cluster as postgresql-common registered it, before its settings are read.
///
/// Deliberately not [`Cluster`](crate::collectors::postgresql::model::Cluster): that type
/// carries settings, and this is what is known before any connection is attempted. Keeping
/// them apart is what lets the whole enumeration be tested from a fixture with no
/// PostgreSQL installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredCluster {
    pub id: ClusterId,
    pub status: ClusterStatus,
    pub port: u16,
    pub owner: String,
}

/// What postgresql-common knows about this box.
pub struct ClusterInventory;

impl ClusterInventory {
    /// Reads `pg_lsclusters` output into the clusters it registered.
    ///
    /// A box with postgresql-common and no cluster prints the header alone, and that is an
    /// empty inventory rather than an error: no cluster created is a real state.
    pub fn parse(output: &str) -> Result<Vec<RegisteredCluster>, CollectionError> {
        let mut clusters = Vec::new();

        for line in output.lines() {
            let columns: Vec<&str> = line.split_whitespace().collect();

            if columns.is_empty() || columns[0] == HEADER_FIRST_COLUMN {
                continue;
            }

            if columns.len() < MINIMUM_COLUMNS {
                return Err(CollectionError::new(format!(
                    "pg_lsclusters printed a row of {} fields where at least \
                     {MINIMUM_COLUMNS} are needed to tell a cluster apart: {line:?}",
                    columns.len()
                )));
            }

            clusters.push(RegisteredCluster {
                id: ClusterId::new(columns[0], columns[1])?,
                port: parse_port(columns[2])?,
                status: ClusterStatus::parse(columns[3])?,
                owner: columns[4].to_owned(),
            });
        }

        Ok(clusters)
    }
}

/// A port is a port, and a cluster whose port cannot be read is not one to guess about.
fn parse_port(column: &str) -> Result<u16, CollectionError> {
    column.parse().map_err(|_| {
        CollectionError::new(format!(
            "pg_lsclusters printed the port {column:?}, which is not a port number"
        ))
    })
}
