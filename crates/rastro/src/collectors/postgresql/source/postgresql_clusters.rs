//! Finding the clusters, and asking each running one what it is configured with.

use std::fs;
use std::io;
use std::path::Path;

use rastro_collector::CollectionError;

use super::cluster_inventory::{ClusterInventory, RegisteredCluster};
use super::postmaster_pid::PostmasterPid;
use super::psql_control_data::PsqlControlData;
use super::psql_database_grants::PsqlDatabaseGrants;
use super::psql_databases::PsqlDatabases;
use super::psql_extensions::PsqlExtensions;
use super::psql_file_settings::PsqlFileSettings;
use super::psql_hba_rules::PsqlHbaRules;
use super::psql_memberships::PsqlMemberships;
use super::psql_read_lens::PsqlReadLens;
use super::psql_role_settings::PsqlRoleSettings;
use super::psql_roles::PsqlRoles;
use super::psql_settings::PsqlSettings;
use crate::collectors::canonical_tool::{CanonicalTool, TargetUser, ToolAsUser};
use crate::collectors::postgresql::model::{
    Cluster, ClusterDatabases, ClusterFileSettings, ClusterHbaRules, ClusterMemberships,
    ClusterRoleSettings, ClusterRoles, ClusterSettings, Clusters, ControlData, Database,
    DatabaseGrants, Postmaster, ReadLens,
};

/// postgresql-common's register of the box.
const INVENTORY_PROGRAM: &str = "pg_lsclusters";

/// The client, run as the cluster's owner.
const CLIENT_PROGRAM: &str = "psql";

/// The file postgresql-common's data directory holds while the server runs.
const PID_FILE: &str = "postmaster.pid";

/// The six columns [`PsqlSettings`] reads, in the order it expects them.
///
/// `context` is selected and not recorded: it says whether a change needs a reload or a
/// restart, which is a property of the setting rather than of this host, and the same on
/// every box running that version. `pending_restart` is the per-host half of that question
/// and is recorded.
const SETTINGS_QUERY: &str =
    "SELECT name, setting, unit, source, context, pending_restart FROM pg_settings";

/// The four columns [`PsqlReadLens`] reads, in the order it expects them.
///
/// The session the settings were read through, not the host: `pg_settings` is a projection
/// of the connecting backend's own GUC array, so which role and database answered decides
/// which `ALTER ... SET` defaults it folded in and whether the `GUC_SUPERUSER_ONLY` rows
/// were dropped. `is_superuser` is cast to a boolean so it reads back as one; `pg_has_role`
/// already answers as one.
const LENS_QUERY: &str = "SELECT current_user, current_database(), \
     current_setting('is_superuser')::boolean, \
     pg_has_role(current_user, 'pg_read_all_settings', 'usage')";

/// The ten columns [`PsqlRoles`] reads, in the order it expects them.
///
/// The attributes come from `pg_roles`, which masks the password. The tenth column is a
/// `CASE` over `pg_authid.rolpassword`, so how a password is stored reaches the document
/// while the hash itself never leaves the server. Reading `pg_authid` at all needs superuser,
/// which the cluster owner is under peer authentication; a cluster whose owner is not will
/// fail this read loudly rather than report roles without saying whether they have passwords.
///
/// The `pg_` roles are filtered out because they arrive with the server version, which the
/// document already records, and the prefix is reserved so nothing an administrator creates
/// can hide behind the filter. `ORDER BY` is deliberately absent: the model sorts, because
/// list order is a contract rastro keeps rather than one it asks the server for.
const ROLES_QUERY: &str = "SELECT r.rolname, r.rolsuper, r.rolcreatedb, r.rolcreaterole, \
     r.rolreplication, r.rolbypassrls, r.rolcanlogin, r.rolconnlimit, r.rolvaliduntil, \
     CASE WHEN a.rolpassword IS NULL THEN '' \
          WHEN a.rolpassword LIKE 'SCRAM-SHA-256$%' THEN 'scram-sha-256' \
          WHEN a.rolpassword LIKE 'md5%' THEN 'md5' \
          ELSE 'unrecognised' END \
     FROM pg_roles r JOIN pg_authid a ON a.oid = r.oid \
     WHERE r.rolname NOT LIKE 'pg\\_%'";

/// The three columns [`PsqlMemberships`] reads, in the order it expects them.
///
/// Only the member side is filtered: `newrelic` in `pg_monitor` is a per-host grant somebody
/// made and is worth having, while `pg_monitor` in `pg_read_all_stats` is a chain the server
/// version ships and is the same everywhere.
const MEMBERSHIPS_QUERY: &str = "SELECT member.rolname, granted.rolname, am.admin_option \
     FROM pg_auth_members am \
     JOIN pg_roles member ON member.oid = am.member \
     JOIN pg_roles granted ON granted.oid = am.roleid \
     WHERE member.rolname NOT LIKE 'pg\\_%'";

/// The five columns [`PsqlDatabases`] reads, in the order it expects them.
///
/// The last is `datacl IS NULL`, and it is asked outright because no rendering of the ACL's
/// contents can answer it: a null array and an empty one both render as nothing, and they
/// are opposite states. Null means the built-in defaults apply; empty means everything has
/// been revoked from everybody.
const DATABASES_QUERY: &str = "SELECT datname, pg_get_userbyid(datdba), datallowconn, \
     datconnlimit, datacl IS NULL FROM pg_database";

/// The five columns [`PsqlDatabaseGrants`] reads, in the order it expects them.
///
/// `aclexplode` rather than the text form of an `aclitem`: a role name may contain a space or
/// an equals sign, so the text form cannot be tokenised. Grantee zero is `PUBLIC` and
/// `pg_get_userbyid` would fail on it, hence the `CASE` rendering it as an empty column.
const DATABASE_GRANTS_QUERY: &str = "SELECT d.datname, \
     CASE WHEN a.grantee = 0 THEN '' ELSE pg_get_userbyid(a.grantee) END, \
     a.privilege_type, a.is_grantable, pg_get_userbyid(a.grantor) \
     FROM pg_database d, aclexplode(d.datacl) a";

/// The three columns [`PsqlExtensions`] reads, in the order it expects them.
///
/// Asked once per connectable database, because `pg_extension` is per database rather than
/// shared. `pg_namespace` gives the schema its objects live in.
const EXTENSIONS_QUERY: &str = "SELECT e.extname, e.extversion, n.nspname FROM pg_extension e \
     JOIN pg_namespace n ON n.oid = e.extnamespace";

/// The three columns [`PsqlRoleSettings`] reads, in the order it expects them.
///
/// `pg_db_role_setting` holds the `ALTER ROLE`/`ALTER DATABASE` defaults `pg_settings`
/// silently folds into one session's map. It is a shared catalog with no `REVOKE`, so this
/// read is cluster-wide from whichever database already answered and needs no privilege.
///
/// `unnest(s.setconfig)` expands the stored `text[]` to one `name=value` per row, so the
/// parser splits a single assignment rather than an array literal. The `coalesce`s turn the
/// null oid of an unscoped default into an empty column. `ORDER BY` is deliberately absent:
/// the model sorts, because list order is a contract rastro keeps rather than one it asks the
/// server for.
const ROLE_SETTINGS_QUERY: &str = "SELECT coalesce(d.datname, ''), coalesce(r.rolname, ''), \
     unnest(s.setconfig) FROM pg_db_role_setting s \
     LEFT JOIN pg_database d ON d.oid = s.setdatabase \
     LEFT JOIN pg_roles r ON r.oid = s.setrole";

/// The seven columns [`PsqlFileSettings`] reads, in the order it expects them.
///
/// `pg_file_settings` re-parses the configuration files, so it sees the half `pg_settings`
/// cannot: a value edited without a reload, and a line that will not apply. Superuser only,
/// which is the privilege the roles read already needs, so it never narrows a readable
/// cluster. `ORDER BY` is deliberately absent: the model sorts by `seqno`, because that order
/// is a contract rastro keeps rather than one it asks the server for.
const FILE_SETTINGS_QUERY: &str = "SELECT seqno, sourcefile, sourceline, name, setting, \
     applied, error FROM pg_file_settings";

/// The two columns [`PsqlControlData`] reads, in the order it expects them.
///
/// The cluster's lineage from `pg_control`: `system_identifier` says which cluster this is,
/// `timeline_id` increments on promotion. The whole `pg_control_*` family is
/// EXECUTE-to-PUBLIC, so this needs no privilege. `system_identifier` is cast to text because
/// it is a 64-bit unsigned value that does not fit an integer column a diff would compute on.
/// Every other column of these functions is a counter that moves on each checkpoint and is
/// deliberately not read.
const CONTROL_QUERY: &str = "SELECT (pg_control_system()).system_identifier::text, \
     (pg_control_checkpoint()).timeline_id";

/// The first major version whose `pg_hba_file_rules` carries `rule_number` and `file_name`.
const HBA_RULE_NUMBER_SINCE: u32 = 16;

/// The eleven columns [`PsqlHbaRules`] reads on PostgreSQL 16 and later.
///
/// Who may connect as whom, from where, and how: server state `pg_settings` does not carry.
/// Superuser only, revoked from PUBLIC. `rule_number` and `file_name` are 16 additions.
/// `ORDER BY` is absent because the model sorts, which is the contract rastro keeps.
const HBA_RULES_QUERY_V16: &str = "SELECT rule_number, file_name, line_number, type, database, \
     user_name, address, netmask, auth_method, options, error FROM pg_hba_file_rules";

/// The nine columns [`PsqlHbaRules`] reads on PostgreSQL 15, which has neither `rule_number`
/// nor `file_name`. Asking for them there would fail the read on the box the collector was
/// written for.
const HBA_RULES_QUERY_V15: &str = "SELECT line_number, type, database, user_name, address, \
     netmask, auth_method, options, error FROM pg_hba_file_rules";

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
        let (read, observed) = if cluster.status.is_online() {
            let observed = self.observe(cluster)?;
            let read = self.query(cluster, observed.as_ref())?;
            (Some(read), observed)
        } else {
            (None, None)
        };

        // Split here rather than held as one option, because the document's shape is one
        // field per catalogue and a stopped cluster leaves every one of them null.
        Ok(Cluster {
            status: cluster.status.clone(),
            port: cluster.port,
            owner: cluster.owner.clone(),
            observed,
            lens: read.as_ref().map(|read| read.lens.clone()),
            settings: read.as_ref().map(|read| read.settings.clone()),
            roles: read.as_ref().map(|read| read.roles.clone()),
            memberships: read.as_ref().map(|read| read.memberships.clone()),
            role_settings: read.as_ref().map(|read| read.role_settings.clone()),
            file_settings: read.as_ref().map(|read| read.file_settings.clone()),
            control: read.as_ref().map(|read| read.control.clone()),
            hba_rules: read.as_ref().map(|read| read.hba_rules.clone()),
            databases: read.map(|read| read.databases),
        })
    }

    /// Reads a running cluster's observed half from `postmaster.pid`.
    ///
    /// **Absent is not a failure.** A register row that named no data directory, or a pid
    /// file that is not there, yields `None`: a cleanly stopped cluster removes the file, and
    /// a row without a directory cannot be pointed at one. Any other read error is loud,
    /// because rastro runs as root and a pid file it cannot read is a fact worth surfacing.
    fn observe(&self, cluster: &RegisteredCluster) -> Result<Option<Postmaster>, CollectionError> {
        let Some(directory) = &cluster.data_directory else {
            return Ok(None);
        };
        let path = Path::new(directory).join(PID_FILE);

        match fs::read_to_string(&path) {
            Ok(content) => Ok(Some(PostmasterPid::parse(&content)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(CollectionError::new(format!(
                "could not read {}: {error}",
                path.display()
            ))),
        }
    }

    /// Asks one running cluster for its effective configuration, as the account that owns it.
    ///
    /// The owner comes from `pg_lsclusters` rather than being assumed to be `postgres`,
    /// which is what [`TargetUser`] validates for: the value reaches sudo as an argument and
    /// was read from the host.
    ///
    /// **The running port is preferred over the configured one.** `pg_lsclusters` prints the
    /// port from the config file, which is stale the moment it is edited without a reload,
    /// while `postmaster.pid` holds the port the server is serving on. Connecting on the
    /// stale one is how a live cluster reads as `down`.
    ///
    /// Every candidate database is tried before the cluster is called unreadable, and the
    /// reasons are reported together: told only that `template1` failed, an operator cannot
    /// see that `postgres` was missing, which is the fact that explains the rest.
    fn query(
        &self,
        cluster: &RegisteredCluster,
        observed: Option<&Postmaster>,
    ) -> Result<ClusterReading, CollectionError> {
        // Validated whichever client is used, so a name read from the host is never trusted.
        let owner = TargetUser::new(cluster.owner.as_str())?;
        let client = match &self.client {
            Some(named) => named.clone(),
            None => ToolAsUser::located(CLIENT_PROGRAM, &owner)?,
        };
        let port = observed
            .map(|observed| observed.port)
            .or(cluster.port)
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "cluster {} is online, but neither pg_lsclusters nor postmaster.pid gave a \
                     port to connect on",
                    cluster.id.as_str()
                ))
            })?
            .to_string();
        // The view gained rule_number and file_name in 16, so an older cluster is asked for
        // the nine columns it has rather than the eleven it does not.
        let hba_query = match cluster.id.major_version() {
            Some(major) if major >= HBA_RULE_NUMBER_SINCE => HBA_RULES_QUERY_V16,
            _ => HBA_RULES_QUERY_V15,
        };
        let mut refusals = Vec::new();

        for database in CANDIDATE_DATABASES {
            match self.query_database(&client, &port, database, hba_query) {
                Ok(reading) => return self.with_extensions(&client, &port, reading),
                Err(refusal) => refusals.push(format!("{database}: {refusal}")),
            }
        }

        Err(CollectionError::new(format!(
            "cluster {} answered on no database tried ({})",
            cluster.id.as_str(),
            refusals.join("; ")
        )))
    }

    /// One connection attempt, against one database, for every catalogue.
    ///
    /// All or nothing per database: both reads are server-wide views that any database can
    /// answer, so one failing where the other succeeded means the connection or the account
    /// is the problem rather than the query. Falling through to the next candidate with half
    /// an answer would report a cluster as partly read without saying which half was missing.
    fn query_database(
        &self,
        client: &ToolAsUser,
        port: &str,
        database: &str,
        hba_query: &str,
    ) -> Result<ClusterReading, CollectionError> {
        Ok(ClusterReading {
            lens: PsqlReadLens::parse(&self.ask(client, port, database, LENS_QUERY)?)?,
            control: PsqlControlData::parse(&self.ask(client, port, database, CONTROL_QUERY)?)?,
            hba_rules: PsqlHbaRules::parse(&self.ask(client, port, database, hba_query)?)?,
            settings: PsqlSettings::parse(&self.ask(client, port, database, SETTINGS_QUERY)?)?,
            file_settings: PsqlFileSettings::parse(&self.ask(
                client,
                port,
                database,
                FILE_SETTINGS_QUERY,
            )?)?,
            roles: PsqlRoles::parse(&self.ask(client, port, database, ROLES_QUERY)?)?,
            role_settings: PsqlRoleSettings::parse(&self.ask(
                client,
                port,
                database,
                ROLE_SETTINGS_QUERY,
            )?)?,
            memberships: PsqlMemberships::parse(&self.ask(
                client,
                port,
                database,
                MEMBERSHIPS_QUERY,
            )?)?,
            databases: Self::joined(
                &PsqlDatabases::parse(&self.ask(client, port, database, DATABASES_QUERY)?)?,
                &PsqlDatabaseGrants::parse(&self.ask(
                    client,
                    port,
                    database,
                    DATABASE_GRANTS_QUERY,
                )?)?,
            )?,
        })
    }

    /// Joins the ACL rows onto the databases that have an ACL.
    ///
    /// Pure, and called from the attempt that read both, so the grants can only ever come
    /// from the database that answered. It used to be a second pass that reconnected to the
    /// first candidate, which meant a cluster with no `postgres` database read everything
    /// through `template1` and then failed here.
    ///
    /// A database whose `datacl` is null keeps `None`: the built-in defaults apply and there
    /// are no rows to join. One that has an ACL gets what `aclexplode` returned, which may be
    /// nothing at all, and nothing at all is a real state rather than a missing read.
    ///
    /// Grants naming a database the register did not list are a failure rather than a shrug:
    /// it means a `CREATE DATABASE` landed between the two queries, so neither answer
    /// describes one moment of the cluster.
    fn joined(
        databases: &ClusterDatabases,
        grants: &DatabaseGrants,
    ) -> Result<ClusterDatabases, CollectionError> {
        let joined = databases
            .databases()
            .iter()
            .map(|database| Database {
                grants: database.grants.as_ref().map(|_| {
                    grants
                        .of_database(database.name.as_str())
                        .unwrap_or_default()
                        .to_vec()
                }),
                ..database.clone()
            })
            .collect::<Vec<Database>>();

        if let Some(unknown) = grants
            .databases()
            .find(|name| !joined.iter().any(|database| &&database.name == name))
        {
            return Err(CollectionError::new(format!(
                "the server reported grants on {:?}, which it did not list as a database, so the \
                 two reads do not describe one moment",
                unknown.as_str()
            )));
        }

        ClusterDatabases::new(joined)
    }

    /// Asks every connectable database what it has installed.
    ///
    /// **A database that refuses connections is left `None` rather than empty.** `template0`
    /// is kept unconnectable on purpose, so nothing can be read from it, and an empty list
    /// would claim it has no extensions when the truth is that nobody asked.
    fn with_extensions(
        &self,
        client: &ToolAsUser,
        port: &str,
        reading: ClusterReading,
    ) -> Result<ClusterReading, CollectionError> {
        let databases = reading
            .databases
            .databases()
            .iter()
            .map(|database| {
                let extensions = if database.allows_connections {
                    Some(PsqlExtensions::parse(&self.ask(
                        client,
                        port,
                        database.name.as_str(),
                        EXTENSIONS_QUERY,
                    )?)?)
                } else {
                    None
                };

                Ok(Database {
                    extensions,
                    ..database.clone()
                })
            })
            .collect::<Result<Vec<Database>, CollectionError>>()?;

        Ok(ClusterReading {
            databases: ClusterDatabases::new(databases)?,
            ..reading
        })
    }

    /// One query, on its own connection.
    ///
    /// A connection per query rather than several `-c` flags on one: psql concatenates the
    /// result sets of multiple commands with nothing to tell them apart, so splitting them
    /// again would mean guessing where one catalogue ends and the next begins.
    fn ask(
        &self,
        client: &ToolAsUser,
        port: &str,
        database: &str,
        query: &str,
    ) -> Result<String, CollectionError> {
        let mut arguments: Vec<&str> = CLIENT_FLAGS.to_vec();

        arguments.extend(["-p", port, "-d", database, "-c", query]);

        client.run(&arguments)
    }
}

/// Everything one connection attempt read.
///
/// A named type rather than a pair, so the caller reads what each half is at the call site.
struct ClusterReading {
    lens: ReadLens,
    settings: ClusterSettings,
    roles: ClusterRoles,
    role_settings: ClusterRoleSettings,
    file_settings: ClusterFileSettings,
    control: ControlData,
    hba_rules: ClusterHbaRules,
    memberships: ClusterMemberships,
    databases: ClusterDatabases,
}
