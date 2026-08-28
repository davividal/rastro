//! Layer 3: what a PostgreSQL cluster is actually running with.
//!
//! The walk from a Layer 2 signal to service-internal state, and the first collector to
//! read a service rather than the kernel or the package manager.
//!
//! **The server's own view, not `postgresql.conf`.** A cluster's effective configuration
//! is not what the file says: an `ALTER SYSTEM`, a command-line override or a value the
//! build defaults to are all invisible in the file, and a file edited without a reload is
//! a value the server is not using. `pg_settings` answers all of that in one place, and
//! carries the two columns a file never can: where each value came from, and whether the
//! running server has taken it up yet.
//!
//! **Keyed by cluster**, because one box legitimately runs several: an upgrade leaves
//! `16/main` and `15/main` side by side, each on its own port with its own effective
//! configuration. The facet holds one entry per cluster rather than the settings of whichever
//! one psql happened to connect to.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    Cluster, ClusterDatabases, ClusterMemberships, ClusterRoleSettings, ClusterRoles,
    ClusterSettings, Clusters, Database, DatabaseExtensions, DatabaseGrants, Extension, Grant,
    Membership, Postmaster, ReadLens, Role, RoleSetting, Setting,
};
pub use source::{
    ClusterInventory, PostgresqlClusters, PostmasterPid, PsqlDatabaseGrants, PsqlDatabases,
    PsqlExtensions, PsqlMemberships, PsqlReadLens, PsqlResultSet, PsqlRoleSettings, PsqlRoles,
    PsqlSettings, RegisteredCluster,
};
pub use value_objects::{
    ClusterId, ClusterStatus, DatabaseName, DatabasePrivilege, ExtensionName, Grantee,
    PasswordMethod, PostmasterStatus, RoleName, SettingName, SettingSource, SettingUnit,
    SettingValue,
};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct PostgresqlCollector {
    name: FacetName,
    identity: CollectorIdentity,
    clusters: Option<PostgresqlClusters>,
}

impl PostgresqlCollector {
    pub fn new() -> Self {
        Self::reading(PostgresqlClusters::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(clusters: Option<PostgresqlClusters>) -> Self {
        Self {
            name: FacetName::new("postgresql").expect("`postgresql` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("postgresql").expect("`postgresql` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            clusters,
        }
    }
}

impl Default for PostgresqlCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for PostgresqlCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `absent` without postgresql-common, `present` with it even if no cluster exists.
    ///
    /// The two are different facts and the document keeps them apart: a box with
    /// postgresql-common and no cluster has had PostgreSQL installed and no cluster created,
    /// which is state, while a box without it has no Debian-managed cluster at all. Neither
    /// is a failure, so neither is `Undetermined`; the reasons rastro genuinely cannot look
    /// (no psql, no sudo, a cluster that refuses the connection) surface from
    /// [`Collector::collect`] as an `error` instead, because by then the cluster is known to
    /// be there.
    fn presence(&self) -> Presence {
        match self.clusters {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let clusters = self.clusters.as_ref().ok_or_else(|| {
            CollectionError::new(
                "no pg_lsclusters was found, so this box has no postgresql-common to ask",
            )
        })?;

        Ok(Observation::from(&clusters.read()?))
    }
}
