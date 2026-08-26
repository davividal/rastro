//! Finding the clusters, and asking each running one what it is configured with.

use rastro_collector::CollectionError;

use super::cluster_inventory::{ClusterInventory, RegisteredCluster};
use super::psql_settings::PsqlSettings;
use crate::collectors::canonical_tool::{CanonicalTool, TargetUser, ToolAsUser};
use crate::collectors::postgresql::model::{Cluster, ClusterSettings, Clusters};

/// postgresql-common's register of the box.
const INVENTORY_PROGRAM: &str = "pg_lsclusters";

/// The client, run as the cluster's owner.
const CLIENT_PROGRAM: &str = "psql";

/// The six columns [`PsqlSettings`] reads, in the order it expects them.
///
/// `context` is selected and not recorded: it says whether a change needs a reload or a
/// restart, which is a property of the setting rather than of this host, and the same on
/// every box running that version. `pending_restart` is the per-host half of that question
/// and is recorded.
const SETTINGS_QUERY: &str =
    "SELECT name, setting, unit, source, context, pending_restart FROM pg_settings";

/// Read nothing of the invoking account's, print no header, quote every value.
///
/// - `-X` because `HOME` belongs to the target account once sudo has built the environment,
///   so a `~/.psqlrc` there could otherwise change the output format out from under the
///   parser.
/// - `-t` because [`PsqlSettings`] parses rows and nothing else; psql's header would arrive
///   as a setting named `name`, and fail on its `pending_restart` column not being a
///   boolean.
/// - `--csv` because a value may contain any separator a person might pick: `archive_command`
///   holds a shell command.
const CLIENT_FLAGS: [&str; 3] = ["-X", "-t", "--csv"];

/// Where to connect, in the order worth trying.
///
/// **`postgres` is a convention, not an invariant.** initdb creates it and an administrator
/// may drop or rename it; nothing in the server requires it. `template1` cannot be dropped,
/// so it is the one name that is always there.
///
/// Ordered rather than reduced to the guaranteed one, because connecting to `template1`
/// blocks a concurrent `CREATE DATABASE` for as long as the session lasts. That is a cost
/// worth paying to read a cluster that would otherwise report a failure, and not worth
/// paying on every box when `postgres` is there. `pg_settings` is a server-wide view, so
/// which database answers makes no difference to what is read.
const CANDIDATE_DATABASES: [&str; 2] = ["postgres", "template1"];

/// The clusters on this box, and the tools to read them with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresqlClusters {
    inventory: CanonicalTool,
    client: Option<ToolAsUser>,
}

impl PostgresqlClusters {
    /// Finds postgresql-common's register, or reports that this box does not have one.
    ///
    /// **`None` is absence, not a failed look, and that is a deliberate narrowing.** Every
    /// Debian `postgresql-server` package depends on postgresql-common, so a box without
    /// `pg_lsclusters` has no Debian-managed cluster. A cluster built from source and
    /// started by hand would be missed, which is a documented gap of the same kind the
    /// exporters facet accepts for an agent systemd does not start: a gap named here is
    /// honest, where reporting every box in the fleet as `error` would be noise that hides
    /// the real failures.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(INVENTORY_PROGRAM).map(Self::using)
    }

    /// The same over a register the caller located, locating a client per cluster owner.
    pub fn using(inventory: CanonicalTool) -> Self {
        Self {
            inventory,
            client: None,
        }
    }

    /// The same with the client named by the caller rather than located per owner.
    ///
    /// The escape hatch that mirrors [`CanonicalTool::located_in`], and the reason the whole
    /// read is testable: with both halves named, the enumeration, the per-cluster branch and
    /// the parse can be exercised against fixtures, on a host with no PostgreSQL and no sudo.
    /// It weakens no guarantee, because whatever is named is still a
    /// [`ToolAsUser`](super::super::super::canonical_tool::ToolAsUser) and still runs under
    /// its bounds.
    pub fn reading_as(inventory: CanonicalTool, client: ToolAsUser) -> Self {
        Self {
            inventory,
            client: Some(client),
        }
    }

    pub fn inventory(&self) -> &CanonicalTool {
        &self.inventory
    }

    /// Enumerates the clusters, then reads the settings of each one that is running.
    pub fn read(&self) -> Result<Clusters, CollectionError> {
        let registered = ClusterInventory::parse(&self.inventory.run(&[])?)?;
        let clusters = registered
            .into_iter()
            .map(|cluster| {
                let read = self.resolve(&cluster)?;

                Ok((cluster.id, read))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Clusters::new(clusters))
    }

    /// Reads one cluster, which for a stopped one means reading nothing.
    ///
    /// **A cluster that is down is not asked, and not an error.** There is no effective
    /// configuration to read: nothing is running to hold one. Falling back to its
    /// `postgresql.conf` would report a file as the state of a server that is not applying
    /// it, which is the substitution this whole collector exists to refuse.
    fn resolve(&self, cluster: &RegisteredCluster) -> Result<Cluster, CollectionError> {
        let settings = if cluster.status.is_online() {
            Some(self.query(cluster)?)
        } else {
            None
        };

        Ok(Cluster {
            status: cluster.status,
            port: cluster.port,
            owner: cluster.owner.clone(),
            settings,
        })
    }

    /// Asks one running cluster for its effective configuration, as the account that owns it.
    ///
    /// The owner comes from `pg_lsclusters` rather than being assumed to be `postgres`,
    /// which is what [`TargetUser`] validates for: the value reaches sudo as an argument and
    /// was read from the host.
    ///
    /// Every candidate database is tried before the cluster is called unreadable, and the
    /// reasons are reported together: told only that `template1` failed, an operator cannot
    /// see that `postgres` was missing, which is the fact that explains the rest.
    fn query(&self, cluster: &RegisteredCluster) -> Result<ClusterSettings, CollectionError> {
        // Validated whichever client is used, so a name read from the host is never trusted.
        let owner = TargetUser::new(cluster.owner.as_str())?;
        let client = match &self.client {
            Some(named) => named.clone(),
            None => ToolAsUser::located(CLIENT_PROGRAM, &owner)?,
        };
        let port = cluster.port.to_string();
        let mut refusals = Vec::new();

        for database in CANDIDATE_DATABASES {
            match self.query_database(&client, &port, database) {
                Ok(settings) => return Ok(settings),
                Err(refusal) => refusals.push(format!("{database}: {refusal}")),
            }
        }

        Err(CollectionError::new(format!(
            "cluster {} answered on no database tried ({})",
            cluster.id.as_str(),
            refusals.join("; ")
        )))
    }

    /// One connection attempt, against one database.
    fn query_database(
        &self,
        client: &ToolAsUser,
        port: &str,
        database: &str,
    ) -> Result<ClusterSettings, CollectionError> {
        let mut arguments: Vec<&str> = CLIENT_FLAGS.to_vec();

        arguments.extend(["-p", port, "-d", database, "-c", SETTINGS_QUERY]);

        PsqlSettings::parse(&client.run(&arguments)?)
    }
}
