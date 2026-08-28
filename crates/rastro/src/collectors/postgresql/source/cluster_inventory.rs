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
/// Seven columns: `Ver Cluster Port Status Owner Datadir Logfile`. Only the first five are
/// read, so a data directory or log file containing a space cannot shift a field that
/// matters. Five, not more, because the owner and the log file can both be empty, and
/// `split_whitespace` collapses an empty field rather than leaving a gap: a cluster whose
/// owner has no passwd entry, and whose log file is unset, prints as few as five tokens.
const MINIMUM_COLUMNS: usize = 5;

/// The header postgresql-common prints before the rows.
const HEADER_FIRST_COLUMN: &str = "Ver";

/// What `pg_lsclusters` prints for a port it could not read from the configuration.
const PORT_UNKNOWN: &str = "<unknown>";

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

    /// The configured port, or `None` where `pg_lsclusters` could not read it.
    ///
    /// It prints `<unknown>` when `postgresql.conf` is unreadable, which is intent rastro
    /// never had and no reason to fail the whole facet over. The running port, when there is
    /// one, comes from `postmaster.pid` instead.
    pub port: Option<u16>,
    pub owner: String,

    /// The data directory, where `postmaster.pid` is read from, or `None` where the row did
    /// not carry one.
    pub data_directory: Option<String>,
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

            let (owner, data_directory) = owner_and_data_directory(&columns[4..]);

            clusters.push(RegisteredCluster {
                id: ClusterId::new(columns[0], columns[1])?,
                status: ClusterStatus::parse(columns[3])?,
                port: parse_port(columns[2])?,
                owner,
                data_directory,
            });
        }

        Ok(clusters)
    }
}

/// A port is a port, and `<unknown>` is not one.
///
/// `pg_lsclusters` prints `<unknown>` when it could not read `postgresql.conf`, which is an
/// absent configured port rather than a garbage one: recorded as `None` so it neither fails
/// the facet nor reads as a real number. Anything else that is not a number is still a
/// failure, because it means the column was misread.
fn parse_port(column: &str) -> Result<Option<u16>, CollectionError> {
    if column == PORT_UNKNOWN {
        return Ok(None);
    }

    column.parse().map(Some).map_err(|_| {
        CollectionError::new(format!(
            "pg_lsclusters printed the port {column:?}, which is neither a port number nor \
             {PORT_UNKNOWN:?}"
        ))
    })
}

/// The owner and the data directory, from the columns after the status.
///
/// `pg_lsclusters` prints the owner from `getpwuid`, which returns nothing when the uid has
/// no passwd entry, so the field is empty and the data directory that follows collapses into
/// its place under whitespace splitting. The data directory is always absolute, so a leading
/// `/` in the owner's column is the tell: the owner was empty, and that column is really the
/// data directory. Recording an empty owner reports the truth (nobody owns this cluster)
/// rather than filing a path under a name a later read would try to `sudo` to.
fn owner_and_data_directory(rest: &[&str]) -> (String, Option<String>) {
    match rest.first() {
        Some(first) if first.starts_with('/') => (String::new(), Some((*first).to_owned())),
        Some(first) => (
            (*first).to_owned(),
            rest.get(1)
                .filter(|directory| directory.starts_with('/'))
                .map(|directory| (*directory).to_owned()),
        ),
        None => (String::new(), None),
    }
}
