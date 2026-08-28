//! Layer 3: what a PostgreSQL cluster is actually running with.
//!
//! The walk from a Layer 2 signal to service-internal state, and the first collector to
//! read a service rather than the kernel or the package manager.
//!
//! **The server's own view, not `postgresql.conf`.** A cluster's effective configuration is
//! not what the file says: an `ALTER SYSTEM`, a command-line override or a value the build
//! defaults to are all invisible in the file, and a file edited without a reload is a value
//! the server is not using. `pg_settings` carries the two columns a file never can: where
//! each value came from, and whether the running server has taken it up yet.
//!
//! **But `pg_settings` is one session's view, not the cluster's**, and the reads around it
//! exist to make that honest. It is a projection of the connecting backend's own GUC array,
//! so it folds in the reading role's and database's `ALTER ROLE`/`ALTER DATABASE` defaults as
//! though they were cluster-wide, and it silently drops the `GUC_SUPERUSER_ONLY` rows for a
//! role that is not privileged to see them. So the settings map is recorded alongside: the
//! `lens` it was read through, `settings_complete` when that lens dropped rows, and
//! `role_settings` from the shared `pg_db_role_setting` catalogue, which is where those
//! per-role and per-database defaults actually live. The configured port and status from
//! `pg_lsclusters` are likewise kept apart from the `observed` half read from
//! `postmaster.pid`, so a stale-config value shows as a disagreement rather than a fact.
//!
//! **Keyed by cluster**, because one box legitimately runs several: an upgrade leaves
//! `16/main` and `15/main` side by side, each on its own port with its own effective
//! configuration. The facet holds one entry per cluster rather than the settings of whichever
//! one psql happened to connect to.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    AvailableExtension, Cluster, ClusterAvailableExtensions, ClusterDatabases, ClusterFileSettings,
    ClusterHbaRules, ClusterMemberships, ClusterReplicationSlots, ClusterRoleSettings,
    ClusterRoles, ClusterSettings, Clusters, ControlData, Database, DatabaseExtensions,
    DatabaseGrants, Extension, FileSetting, Grant, HbaRule, Membership, Postmaster, ReadLens,
    ReplicationSlot, Role, RoleSetting, Setting,
};
pub use source::{
    ClusterInventory, PostgresqlClusters, PostmasterPid, PsqlAvailableExtensions, PsqlControlData,
    PsqlDatabaseGrants, PsqlDatabases, PsqlExtensions, PsqlFileSettings, PsqlHbaRules,
    PsqlMemberships, PsqlReadLens, PsqlReplicationSlots, PsqlResultSet, PsqlRoleSettings,
    PsqlRoles, PsqlSettings, RegisteredCluster,
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
