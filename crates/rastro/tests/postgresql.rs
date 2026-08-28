//! Reading a cluster's effective settings, without needing a cluster to read them from.
//!
//! Every fixture here is output psql really produced on a PostgreSQL 17 cluster, which is
//! why the odd-looking ones are in: `"""$user"", public"` is how `search_path` arrives,
//! and `log_line_prefix` really does end in a space.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::canonical_tool::{CanonicalTool, TargetUser as ClusterOwner, ToolAsUser};
use rastro::collectors::postgresql::{
    Cluster, ClusterId, ClusterInventory, ClusterStatus, Clusters, PostgresqlClusters,
    PostgresqlCollector, PostmasterStatus, PsqlAvailableExtensions, PsqlControlData, PsqlDatabases,
    PsqlFileSettings, PsqlHbaRules, PsqlMemberships, PsqlReadLens, PsqlReplicationSlots,
    PsqlRoleSettings, PsqlRoles, PsqlSettings, RegisteredCluster, Setting, SettingName,
    SettingSource,
};
use rastro_collector::{Collector, Observation, Presence};
use support::fs_tree::scratch_tree;
use support::observation::{boolean, field, integer, is_null, items_of, keys_of, text};

/// The eight columns the collector asks for, as psql renders them with `--csv -t`. The last
/// two are the source file and line, empty for a value that came from no file.
const SETTINGS: &str = "\
DateStyle,\"ISO, MDY\",,configuration file,user,f,,
log_line_prefix,%m [%p] %q%u@%d ,,configuration file,sighup,f,/etc/postgresql/17/main/postgresql.conf,100
max_connections,100,,configuration file,postmaster,f,/etc/postgresql/17/main/postgresql.conf,64
search_path,\"\"\"$user\"\", public\",,default,user,f,,
shared_buffers,16384,8kB,configuration file,postmaster,f,/etc/postgresql/17/main/postgresql.conf,120
";

/// The three columns the extensions query asks for, once per connectable database.
const EXTENSIONS: &str = "\
plpgsql,1.0,pg_catalog
";

/// The five columns the databases query asks for, with a grant to a migration role.
const DATABASES: &str = "\
postgres,postgres,t,-1,t
orders,postgres,t,-1,f
";

/// The five columns the grants query asks for, as `aclexplode` answers them. The empty
/// grantee is `PUBLIC`.
const DATABASE_GRANTS: &str = "\
orders,,CONNECT,f,postgres
orders,migrator,CREATE,t,postgres
";

/// The three columns the memberships query asks for: the two monitoring accounts that box
/// holds in `pg_monitor`, and a developer in the migration role.
const MEMBERSHIPS: &str = "\
newrelic,pg_monitor,f
developer,migrator,f
";

/// The nine columns the roles query asks for. `ops_admin` is real: that box carries
/// two superusers besides the cluster owner.
const ROLES: &str = "\
ops_admin,t,t,t,t,f,t,-1,,scram-sha-256
postgres,t,t,t,t,t,t,-1,,scram-sha-256
app,f,f,f,f,f,t,-1,,scram-sha-256
migrator,f,f,f,f,f,t,-1,,md5
";

/// The three columns the role-settings query asks for: an `ALTER ROLE`, an `ALTER DATABASE`
/// and an `ALTER ROLE ... IN DATABASE`, which is every scope `pg_db_role_setting` records.
const ROLE_SETTINGS: &str = "\
,app,work_mem=256MB
orders,,statement_timeout=5000
orders,migrator,search_path=public
";

/// The four columns the lens query asks for: the superuser owner rastro connects as under
/// peer authentication, in the database that answered.
const LENS: &str = "\
postgres,postgres,t,t
";

/// The seven columns the file-settings query asks for: a plain line, a credential-bearing
/// one, and a drop-in that will not apply, which is the typo a file comparison reads past.
const FILE_SETTINGS: &str = "\
1,/etc/postgresql/17/main/postgresql.conf,110,max_connections,100,t,
2,/etc/postgresql/17/main/postgresql.conf,112,archive_command,rsync %p backup:%f,t,
3,/etc/postgresql/17/main/conf.d/10-bad.conf,4,shared_buffers,notasize,f,invalid value for parameter shared_buffers
";

/// The two columns the control query asks for: the system identifier (a 64-bit value carried
/// as text) and the timeline.
const CONTROL: &str = "\
7280634931331371930,1
";

/// The eleven columns the PostgreSQL 16+ hba query asks for: a peer local rule and a
/// scram host rule, with the array columns rendered as `{...}`.
const HBA_RULES: &str = "\
1,/etc/postgresql/17/main/pg_hba.conf,90,local,{all},{postgres},,,peer,{},
2,/etc/postgresql/17/main/pg_hba.conf,92,host,{all},{all},127.0.0.1/32,,scram-sha-256,{},
";

/// The three columns the available-extensions query asks for: one installed, and one that is
/// available but not created in the database that answered.
const AVAILABLE_EXTENSIONS: &str = "\
plpgsql,1.0,1.0
pg_stat_statements,1.11,
";

/// The six stable columns the replication-slots query asks for: a physical slot and a
/// logical one, the latter bound to a database and decoding two-phase commits.
const REPLICATION_SLOTS: &str = "\
standby_1,,physical,,f,f
sub_slot,pgoutput,logical,orders,f,t
";

fn parsed(csv: &str) -> Vec<Setting> {
    PsqlSettings::parse(csv)
        .expect("this output is well formed")
        .settings()
        .to_vec()
}

fn named<'a>(settings: &'a [Setting], name: &str) -> &'a Setting {
    settings
        .iter()
        .find(|setting| setting.name == SettingName::new(name).expect("a legal name"))
        .expect("the fixture has this setting")
}

#[test]
fn parse_reads_every_column_of_a_setting() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert
    let shared_buffers = named(&settings, "shared_buffers");
    assert_eq!(shared_buffers.value.as_str(), "16384");
    assert_eq!(
        shared_buffers.unit.as_ref().map(|unit| unit.as_str()),
        Some("8kB")
    );
    assert_eq!(
        shared_buffers.source,
        SettingSource::new("configuration file").expect("a legal source")
    );
    assert!(!shared_buffers.pending_restart);
}

#[test]
fn parse_reads_the_context_and_source_location_of_a_setting() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: context tells whether a changed value could have taken effect, and the source
    // file and line say where it was set.
    let shared_buffers = named(&settings, "shared_buffers");
    assert_eq!(shared_buffers.context, "postmaster");
    assert_eq!(
        shared_buffers.sourcefile.as_deref(),
        Some("/etc/postgresql/17/main/postgresql.conf")
    );
    assert_eq!(shared_buffers.sourceline, Some(120));

    // A value that came from no file has neither a source file nor a line.
    let search_path = named(&settings, "search_path");
    assert_eq!(search_path.sourcefile, None);
    assert_eq!(search_path.sourceline, None);
}

#[test]
fn parse_keeps_a_quoted_value_whole() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: the comma inside `ISO, MDY` is part of the value, not a column break.
    assert_eq!(named(&settings, "DateStyle").value.as_str(), "ISO, MDY");
}

#[test]
fn parse_reads_a_doubled_quote_as_one_quote() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: RFC 4180 escaping, which is how `search_path` arrives every time.
    assert_eq!(
        named(&settings, "search_path").value.as_str(),
        "\"$user\", public"
    );
}

#[test]
fn parse_preserves_trailing_space_in_a_value() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: trimming would change what the server would actually log, and the
    // operator who put that space there meant it.
    assert_eq!(
        named(&settings, "log_line_prefix").value.as_str(),
        "%m [%p] %q%u@%d "
    );
}

#[test]
fn a_setting_that_can_hold_a_credential_is_redacted() {
    // Arrange: `primary_conninfo` carries `password=` on a standby set up inline, and is
    // visible precisely because rastro connects as the superuser owner. The server redacts
    // none of it, so the collector must.
    let standby = "\
max_connections,100,,configuration file,postmaster,f,,
primary_conninfo,host=primary user=replicator password=hunter2,,configuration file,postmaster,f,,
";

    // Act
    let observation =
        Observation::from(&PsqlSettings::parse(standby).expect("this output is well formed"));

    // Assert: the value is withheld by name, while an ordinary setting beside it is not.
    assert_eq!(
        field(&field(&observation, "primary_conninfo"), "value").sensitivity(),
        rastro_fingerprint::Sensitivity::Sensitive
    );
    assert_eq!(
        field(&field(&observation, "max_connections"), "value").sensitivity(),
        rastro_fingerprint::Sensitivity::Public
    );
}

#[test]
fn parse_reads_an_empty_unit_as_no_unit() {
    // Act
    let settings = parsed(SETTINGS);

    // Assert: 303 of a cluster's 379 settings have no unit, and psql renders both an
    // empty string and a null as nothing at all.
    assert_eq!(named(&settings, "max_connections").unit, None);
}

#[test]
fn parse_reads_a_setting_awaiting_a_restart() {
    // Arrange
    let changed = "max_connections,200,,configuration file,postmaster,t,,\n";

    // Act
    let settings = parsed(changed);

    // Assert: a value the server has read and cannot adopt without restarting. It says
    // nothing about a file edited and never reloaded, which the server does not know about
    // and which this column reports as `false`.
    assert!(named(&settings, "max_connections").pending_restart);
}

#[test]
fn parse_drops_a_setting_that_came_from_our_own_connection() {
    // Arrange: psql sets `application_name` on the connection the collector opens, so
    // the server reports it with source `client`.
    let ours = "\
application_name,psql,,client,user,f,,
max_connections,100,,configuration file,postmaster,f,,
";

    // Act
    let settings = parsed(ours);

    // Assert: recording it would put rastro's own connection into the fingerprint and
    // call it the state of the host.
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].name.as_str(), "max_connections");
}

#[test]
fn parse_reads_a_value_containing_a_newline() {
    // Arrange: `archive_command` holds a shell command, so nothing stops it spanning
    // lines. A row-per-line parser would read this as two broken rows.
    let multiline = "archive_command,\"test ! -f x &&\ncp %p x\",,configuration file,sighup,f,,\n";

    // Act
    let settings = parsed(multiline);

    // Assert
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].value.as_str(), "test ! -f x &&\ncp %p x");
}

#[test]
fn parse_orders_settings_by_name() {
    // Arrange
    let unsorted = "\
shared_buffers,16384,8kB,configuration file,postmaster,f,,
DateStyle,\"ISO, MDY\",,configuration file,user,f,,
max_connections,100,,configuration file,postmaster,f,,
";

    // Act
    let settings = parsed(unsorted);

    // Assert: the document's list order is contractual, so it is decided here rather
    // than inherited from whatever order the server answered in.
    let names: Vec<&str> = settings
        .iter()
        .map(|setting| setting.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["DateStyle", "max_connections", "shared_buffers"]
    );
}

#[test]
fn parse_refuses_a_row_with_the_wrong_number_of_columns() {
    // Act
    let refused = PsqlSettings::parse("max_connections,100,,configuration file\n");

    // Assert: a short row means the query and this parser disagree about the columns,
    // and guessing which one is missing would put a value under the wrong name.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_output_with_no_settings_in_it() {
    // Act
    let refused = PsqlSettings::parse("\n");

    // Assert: every cluster has hundreds of settings, so nothing at all is a failed
    // read rather than a cluster with no configuration.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_two_rows_for_one_setting() {
    // Arrange
    let contradiction = "\
max_connections,100,,configuration file,postmaster,f,,
max_connections,200,,configuration file,postmaster,f,,
";

    // Act
    let refused = PsqlSettings::parse(contradiction);

    // Assert
    assert!(refused.is_err());
}

// ---------------------------------------------------------------------------
// The clusters on the box: enumeration, keying, and the facet they produce.
// ---------------------------------------------------------------------------

/// `pg_lsclusters` as postgresql-common prints it on a box mid-upgrade: the new cluster
/// running, the old one kept until the data is dropped, and a third owned by somebody other
/// than `postgres` because nothing requires that name.
const REGISTERED: &str = "\
Ver Cluster Port Status Owner    Data directory              Log file
16  main    5432 online postgres /var/lib/postgresql/16/main /var/log/postgresql/postgresql-16-main.log
15  main    5433 down   postgres /var/lib/postgresql/15/main /var/log/postgresql/postgresql-15-main.log
9   legacy  5434 online pgsql    /var/lib/postgresql/9/legacy /var/log/postgresql/postgresql-9-legacy.log
";

fn registered(output: &str) -> Vec<RegisteredCluster> {
    ClusterInventory::parse(output).expect("this output is well formed")
}

#[test]
fn parse_reads_every_registered_cluster() {
    // Act
    let clusters = registered(REGISTERED);

    // Assert
    assert_eq!(clusters.len(), 3);
    assert_eq!(clusters[0].id.as_str(), "16/main");
    assert_eq!(clusters[0].port, Some(5432));
    assert_eq!(clusters[0].owner, "postgres");
    assert!(clusters[0].status.is_online());
}

#[test]
fn parse_reads_the_owner_rather_than_assuming_postgres() {
    // Act
    let clusters = registered(REGISTERED);

    // Assert: peer authentication refuses everybody but the owner, so guessing this name
    // would turn a readable cluster into a reported failure.
    let legacy = &clusters[2];
    assert_eq!(legacy.id.as_str(), "9/legacy");
    assert_eq!(legacy.owner, "pgsql");
}

#[test]
fn parse_tells_a_stopped_cluster_from_a_running_one() {
    // Act
    let clusters = registered(REGISTERED);

    // Assert
    assert!(!clusters[1].status.is_online());
}

#[test]
fn parse_reads_a_box_with_no_cluster_created_as_an_empty_inventory() {
    // Arrange: postgresql-common installed, nothing created. Only the header prints.
    let header = "Ver Cluster Port Status Owner Data directory Log file\n";

    // Act & Assert: no cluster is state, not a failure.
    assert!(registered(header).is_empty());
}

#[test]
fn parse_refuses_a_row_it_cannot_tell_a_cluster_from() {
    // Act
    let refused = ClusterInventory::parse("16 main 5432\n");

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_a_status_that_is_neither_online_nor_down() {
    // Act: the facet branches on this, so a word rastro does not know cannot be guessed.
    let refused =
        ClusterInventory::parse("16 main 5432 wedged postgres /var/lib/pg /var/log/pg.log\n");

    // Assert
    assert!(refused.is_err());
}

#[test]
fn clusters_render_in_one_deterministic_key_order() {
    // Arrange
    let clusters = registered(REGISTERED);
    let fleet = Clusters::new(clusters.into_iter().map(|cluster| {
        (
            cluster.id,
            Cluster {
                status: cluster.status,
                port: cluster.port,
                owner: cluster.owner,
                settings: None,
                roles: None,
                memberships: None,
                role_settings: None,
                file_settings: None,
                control: None,
                hba_rules: None,
                replication_slots: None,
                available_extensions: None,
                observed: None,
                lens: None,
                databases: None,
            },
        )
    }));

    // Act
    let keys = keys_of(&Observation::from(&fleet));

    // Assert: lexicographic on the key text, because an `Observation` object is a
    // `BTreeMap<String, _>` and the format owns that ordering. `15/main` before `9/legacy`
    // reads oddly and is still the contract: what it must be is the same every run.
    assert_eq!(keys, vec!["15/main", "16/main", "9/legacy"]);
}

#[test]
fn a_running_cluster_carries_its_settings_and_a_stopped_one_carries_none() {
    // Arrange
    let running = Cluster {
        status: ClusterStatus::parse("online").expect("a legal status"),
        port: Some(5432),
        owner: "postgres".to_owned(),
        settings: Some(PsqlSettings::parse(SETTINGS).expect("well formed")),
        roles: Some(PsqlRoles::parse(ROLES).expect("well formed")),
        memberships: Some(PsqlMemberships::parse(MEMBERSHIPS).expect("well formed")),
        role_settings: Some(PsqlRoleSettings::parse(ROLE_SETTINGS).expect("well formed")),
        file_settings: Some(PsqlFileSettings::parse(FILE_SETTINGS).expect("well formed")),
        control: Some(PsqlControlData::parse(CONTROL).expect("well formed")),
        hba_rules: Some(PsqlHbaRules::parse(HBA_RULES).expect("well formed")),
        replication_slots: Some(
            PsqlReplicationSlots::parse(REPLICATION_SLOTS).expect("well formed"),
        ),
        available_extensions: Some(
            PsqlAvailableExtensions::parse(AVAILABLE_EXTENSIONS).expect("well formed"),
        ),
        observed: None,
        lens: Some(PsqlReadLens::parse(LENS).expect("well formed")),
        databases: Some(PsqlDatabases::parse(DATABASES).expect("well formed")),
    };
    let stopped = Cluster {
        status: ClusterStatus::parse("down").expect("a legal status"),
        port: Some(5433),
        owner: "postgres".to_owned(),
        settings: None,
        roles: None,
        memberships: None,
        role_settings: None,
        file_settings: None,
        control: None,
        hba_rules: None,
        replication_slots: None,
        available_extensions: None,
        observed: None,
        lens: None,
        databases: None,
    };
    let fleet = Clusters::new([
        (ClusterId::new("16", "main").expect("legal"), running),
        (ClusterId::new("15", "main").expect("legal"), stopped),
    ]);

    // Act
    let observation = Observation::from(&fleet);

    // Assert: a stopped cluster has no effective configuration, because nothing is running
    // to hold one. Null says that; an empty object would read as a cluster configured with
    // nothing.
    let sixteen = field(&observation, "16/main");
    assert_eq!(text(&field(&sixteen, "status")), "online");
    assert_eq!(integer(&field(&sixteen, "port")), 5432);
    assert_eq!(text(&field(&sixteen, "owner")), "postgres");
    assert!(!is_null(&field(&sixteen, "settings")));

    let fifteen = field(&observation, "15/main");
    assert_eq!(text(&field(&fifteen, "status")), "down");
    assert!(is_null(&field(&fifteen, "settings")));
}

#[test]
fn the_collector_reports_absent_without_postgresql_common() {
    // Arrange: no pg_lsclusters means no Debian-managed cluster.
    let collector = PostgresqlCollector::reading(None);

    // Act & Assert: absence, not a failed look. Reporting `error` on every box in a fleet
    // that has no PostgreSQL would bury the failures that are real.
    assert_eq!(collector.presence(), Presence::Absent);
    assert!(collector.collect().is_err());
}

#[test]
fn the_collector_names_the_facet_it_fills() {
    // Arrange
    let collector = PostgresqlCollector::reading(None);

    // Act & Assert
    assert_eq!(collector.name().as_str(), "postgresql");
    assert_eq!(collector.identity().id.as_str(), "postgresql");
}

#[test]
fn parse_reads_a_standby_as_running_and_in_recovery() {
    // Arrange: pg_lsclusters appends `,recovery` when it finds recovery.signal or
    // recovery.conf in the data directory, which is every standby on the box.
    let standby = "\
Ver Cluster Port Status         Owner    Data directory              Log file
17  main    5432 online,recovery postgres /var/lib/postgresql/17/main /var/log/pg.log
";

    // Act
    let clusters = registered(standby);

    // Assert: a standby is running and readable, so treating the suffix as an unknown
    // status would fail the whole facet on every replica in the fleet.
    assert_eq!(clusters.len(), 1);
    assert!(clusters[0].status.is_online());
    assert!(clusters[0].status.in_recovery());
}

#[test]
fn parse_reads_a_stopped_standby_as_neither_running_nor_lost() {
    // Arrange
    let stopped = "17 main 5432 down,recovery postgres /var/lib/pg /var/log/pg.log\n";

    // Act
    let clusters = registered(stopped);

    // Assert
    assert!(!clusters[0].status.is_online());
    assert!(clusters[0].status.in_recovery());
}

#[test]
fn recovery_reaches_the_facet_as_its_own_fact() {
    // Arrange
    let cluster = Cluster {
        status: ClusterStatus::parse("online,recovery").expect("a legal status"),
        port: Some(5432),
        owner: "postgres".to_owned(),
        settings: None,
        roles: None,
        memberships: None,
        role_settings: None,
        file_settings: None,
        control: None,
        hba_rules: None,
        replication_slots: None,
        available_extensions: None,
        observed: None,
        lens: None,
        databases: None,
    };
    let fleet = Clusters::new([(ClusterId::new("17", "main").expect("legal"), cluster)]);

    // Act
    let observation = field(&Observation::from(&fleet), "17/main");

    // Assert: a cluster promoted out of recovery, or demoted into it, is exactly the change
    // a fingerprint exists to catch, so it is a field rather than a word inside `status`.
    assert_eq!(text(&field(&observation, "status")), "online");
    assert!(boolean(&field(&observation, "in_recovery")));
}

#[test]
fn parse_records_a_missing_package_without_failing_the_facet() {
    // Arrange: a purged old-version package leaves this after a major upgrade, and it is
    // exactly the state a fingerprint should be showing.
    let purged = "15 main 5433 down,binaries_missing postgres /var/lib/pg /var/log/pg.log\n";

    // Act
    let clusters = registered(purged);

    // Assert: the qualifier is recorded, not rejected, so this row does not take the whole
    // facet, and every other cluster on the box, down with it.
    assert!(!clusters[0].status.is_online());
    assert_eq!(clusters[0].status.qualifiers(), ["binaries_missing"]);
}

#[test]
fn parse_records_a_supervisor_alongside_recovery() {
    // Arrange: a Patroni-managed standby stacks the qualifiers onto the running word.
    let managed = "17 main 5432 online,recovery,patroni postgres /var/lib/pg /var/log/pg.log\n";

    // Act
    let clusters = registered(managed);

    // Assert: recovery is its own fact, and the supervisor is kept rather than rejected.
    assert!(clusters[0].status.is_online());
    assert!(clusters[0].status.in_recovery());
    assert_eq!(clusters[0].status.qualifiers(), ["patroni"]);
}

#[test]
fn parse_reads_an_empty_owner_column_as_no_owner() {
    // Arrange: a uid with no passwd entry prints an empty owner, and the data directory
    // collapses into its place under whitespace splitting.
    let orphaned = "17 main 5432 down /var/lib/postgresql/17/main /var/log/pg.log\n";

    // Act
    let clusters = registered(orphaned);

    // Assert: the path is not filed as the owner. Empty is the truth, rather than a name a
    // later read would try to sudo to.
    assert_eq!(clusters[0].owner, "");
}

#[test]
fn a_status_qualifier_reaches_the_facet_as_its_own_list() {
    // Arrange
    let cluster = Cluster {
        status: ClusterStatus::parse("down,binaries_missing").expect("a legal status"),
        port: Some(5433),
        owner: "postgres".to_owned(),
        lens: None,
        settings: None,
        roles: None,
        memberships: None,
        role_settings: None,
        file_settings: None,
        control: None,
        hba_rules: None,
        replication_slots: None,
        available_extensions: None,
        observed: None,
        databases: None,
    };
    let fleet = Clusters::new([(ClusterId::new("15", "main").expect("legal"), cluster)]);

    // Act
    let observation = field(&Observation::from(&fleet), "15/main");

    // Assert: a package going missing, or a supervisor appearing, shows in a diff without
    // being able to fail the read.
    let qualifiers = items_of(&field(&observation, "qualifiers"));
    assert_eq!(qualifiers.len(), 1);
    assert_eq!(text(&qualifiers[0]), "binaries_missing");
}

// ---------------------------------------------------------------------------
// The whole read, against fake tools: no cluster, no psql, no sudo.
// ---------------------------------------------------------------------------

/// A shell script standing in for a host tool, so the composition can be exercised where
/// the host would otherwise decide the answer.
fn fake_tool(tree: &str, program: &str, body: &str) -> CanonicalTool {
    let root = scratch_tree(&format!("postgresql-{tree}"), &[]);
    let path = root.join(program);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("an executable script");
    CanonicalTool::located_in(program, &[root.to_str().expect("utf-8 scratch path")])
        .expect("the script should be locatable")
}

fn fake_inventory(tree: &str, listed: &str) -> CanonicalTool {
    fake_tool(
        tree,
        "pg_lsclusters",
        &format!("cat <<'EOF'\n{listed}\nEOF"),
    )
}

/// Stands in for `sudo`: drops `-n -u <user>` and runs the rest, so the delegation is real
/// while the privilege drop is not.
fn fake_client(tree: &str, psql_body: &str) -> ToolAsUser {
    ToolAsUser::using(
        fake_tool(&format!("{tree}-sudo"), "sudo", "shift 3\nexec \"$@\""),
        fake_tool(&format!("{tree}-psql"), "psql", psql_body),
        ClusterOwner::new("postgres").expect("a legal user name"),
    )
}

fn read_with(tree: &str, listed: &str, psql_body: &str) -> Clusters {
    PostgresqlClusters::reading_as(fake_inventory(tree, listed), fake_client(tree, psql_body))
        .read()
        .expect("these fixtures are well formed")
}

/// A psql that answers each catalogue the collector asks for, dispatching on the query it
/// was handed.
///
/// The collector sends more than one query per connection, so a stand-in that answered
/// everything with one fixture would feed a settings result set to the roles parser and fail
/// for a reason the test is not about.
///
/// `pg_auth_members` is matched before `pg_roles` on purpose: the memberships query joins
/// `pg_roles` twice, so the looser pattern would answer it with the roles fixture.
fn answering_every_query() -> String {
    format!(
        "case \"$*\" in\n\
         *pg_read_all_settings*) cat <<'EOF'\n{LENS}EOF\n ;;\n\
         *pg_db_role_setting*) cat <<'EOF'\n{ROLE_SETTINGS}EOF\n ;;\n\
         *pg_file_settings*) cat <<'EOF'\n{FILE_SETTINGS}EOF\n ;;\n\
         *pg_control_system*) cat <<'EOF'\n{CONTROL}EOF\n ;;\n\
         *pg_hba_file_rules*) cat <<'EOF'\n{HBA_RULES}EOF\n ;;\n\
         *pg_available_extensions*) cat <<'EOF'\n{AVAILABLE_EXTENSIONS}EOF\n ;;\n\
         *pg_replication_slots*) cat <<'EOF'\n{REPLICATION_SLOTS}EOF\n ;;\n\
         *pg_settings*) cat <<'EOF'\n{SETTINGS}EOF\n ;;\n\
         *pg_auth_members*) cat <<'EOF'\n{MEMBERSHIPS}EOF\n ;;\n\
         *aclexplode*) cat <<'EOF'\n{DATABASE_GRANTS}EOF\n ;;\n\
         *pg_database*) cat <<'EOF'\n{DATABASES}EOF\n ;;\n\
         *pg_extension*) cat <<'EOF'\n{EXTENSIONS}EOF\n ;;\n\
         *pg_roles*) cat <<'EOF'\n{ROLES}EOF\n ;;\n\
         *) printf 'unexpected query: %s\\n' \"$*\" >&2; exit 3 ;;\n\
         esac"
    )
}

#[test]
fn read_asks_a_running_cluster_and_carries_its_settings() {
    // Act
    let clusters = read_with(
        "online",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: the whole path, from the register through the delegated client to the parse.
    assert_eq!(clusters.len(), 1);
    assert!(!clusters.is_empty());
    let settings = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .settings
        .as_ref()
        .expect("a running cluster is asked");
    assert_eq!(settings.settings().len(), 5);
}

#[test]
fn read_does_not_ask_a_stopped_cluster() {
    // Arrange: a client that fails if it is ever invoked, so being asked is a test failure
    // rather than a silent success.
    let clusters = read_with(
        "stopped",
        "17  main    5432 down   postgres /var/lib/pg /var/log/pg.log",
        "printf 'a stopped cluster must not be queried\\n' >&2\nexit 1",
    );

    // Assert
    assert!(
        clusters
            .clusters()
            .values()
            .next()
            .expect("one cluster")
            .settings
            .is_none()
    );
}

#[test]
fn read_carries_a_running_clusters_roles() {
    // Act
    let clusters = read_with(
        "roles",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: the catalogue read the settings-only collector was missing. A
    // second superuser is in the fixture because that is the change worth making loud.
    let roles = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .roles
        .as_ref()
        .expect("a running cluster is asked for its roles");
    assert_eq!(roles.roles().len(), 4);
    assert_eq!(
        roles
            .roles()
            .iter()
            .filter(|role| role.superuser)
            .map(|role| role.name.as_str())
            .collect::<Vec<&str>>(),
        vec!["ops_admin", "postgres"]
    );
}

#[test]
fn read_carries_a_running_clusters_role_settings() {
    // Act
    let clusters = read_with(
        "role-settings",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: the `ALTER ROLE`/`ALTER DATABASE` overrides `pg_settings` folds into one
    // session's map are read apart, so a diff can tell a scoped default from a global one.
    let role_settings = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .role_settings
        .as_ref()
        .expect("a running cluster is asked for its overrides");
    assert_eq!(role_settings.settings().len(), 3);
}

#[test]
fn read_carries_a_running_clusters_file_settings() {
    // Act
    let clusters = read_with(
        "file-settings",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: the configuration-file lines are read apart from the effective settings, so a
    // value edited without a reload, and a line that will not apply, are both visible.
    let file_settings = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .file_settings
        .as_ref()
        .expect("a running cluster is asked for its file settings");
    assert_eq!(file_settings.settings().len(), 3);
}

#[test]
fn read_carries_a_running_clusters_control_lineage() {
    // Act
    let clusters = read_with(
        "control",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: the control-file lineage neither pg_settings nor pg_lsclusters can produce, so
    // a re-initdb or a promotion shows in a diff.
    let control = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .control
        .as_ref()
        .expect("a running cluster is asked for its lineage");
    assert_eq!(control.system_identifier, "7280634931331371930");
    assert_eq!(control.timeline_id, 1);
}

#[test]
fn read_carries_a_running_clusters_hba_rules() {
    // Act
    let clusters = read_with(
        "hba",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: who may connect as whom is server state pg_settings does not carry, so the
    // rules are read apart.
    let hba_rules = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .hba_rules
        .as_ref()
        .expect("a running cluster is asked for its authentication rules");
    assert_eq!(hba_rules.rules().len(), 2);
}

#[test]
fn read_carries_a_running_clusters_available_extensions() {
    // Act
    let clusters = read_with(
        "available-extensions",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: what is installable cluster-wide, distinct from what each database created.
    let available = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .available_extensions
        .as_ref()
        .expect("a running cluster is asked for its available extensions");
    assert_eq!(available.extensions().len(), 2);
}

#[test]
fn read_carries_a_running_clusters_replication_slots() {
    // Act
    let clusters = read_with(
        "replication-slots",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: a slot appearing is a subscription pointed at this cluster, so the stable
    // subset is read.
    let slots = clusters
        .clusters()
        .values()
        .next()
        .expect("one cluster")
        .replication_slots
        .as_ref()
        .expect("a running cluster is asked for its replication slots");
    assert_eq!(slots.slots().len(), 2);
}

#[test]
fn read_carries_the_lens_the_settings_were_read_through() {
    // Act
    let clusters = read_with(
        "lens",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &answering_every_query(),
    );

    // Assert: peer authentication as the owner lands on a superuser, so the map is complete
    // and the reader can trust it carries the GUC_SUPERUSER_ONLY rows.
    let cluster = clusters.clusters().values().next().expect("one cluster");
    let lens = cluster
        .lens
        .as_ref()
        .expect("a running cluster records its lens");
    assert!(lens.is_superuser);
    assert!(lens.sees_all_settings());
}

#[test]
fn a_non_privileged_read_marks_the_settings_incomplete() {
    // Arrange: a cluster read as a role that is neither a superuser nor a member of
    // pg_read_all_settings, which is the "not always a superuser" case peer auth allows.
    let cluster = Cluster {
        status: ClusterStatus::parse("online").expect("a legal status"),
        port: Some(5432),
        owner: "postgres".to_owned(),
        lens: Some(PsqlReadLens::parse("app,orders,f,f\n").expect("well formed")),
        settings: Some(PsqlSettings::parse(SETTINGS).expect("well formed")),
        roles: None,
        memberships: None,
        role_settings: None,
        file_settings: None,
        control: None,
        hba_rules: None,
        replication_slots: None,
        available_extensions: None,
        observed: None,
        databases: None,
    };
    let fleet = Clusters::new([(ClusterId::new("16", "main").expect("legal"), cluster)]);

    // Act
    let observation = field(&Observation::from(&fleet), "16/main");

    // Assert: the map is present but flagged incomplete, because the 21 GUC_SUPERUSER_ONLY
    // rows are dropped with no word from the server. The flag is the loud qualifier.
    assert!(!is_null(&field(&observation, "settings")));
    assert!(!boolean(&field(&observation, "settings_complete")));
}

#[test]
fn read_does_not_ask_a_stopped_cluster_for_roles() {
    // Act
    let clusters = read_with(
        "stopped-roles",
        "17  main    5432 down   postgres /var/lib/pg /var/log/pg.log",
        "printf 'a stopped cluster must not be queried\\n' >&2\nexit 1",
    );

    // Assert: a cluster that is down has no catalogue to read for the same reason it has no
    // effective configuration. Nothing is running to hold either.
    assert!(
        clusters
            .clusters()
            .values()
            .next()
            .expect("one cluster")
            .roles
            .is_none()
    );
}

#[test]
fn read_falls_back_to_the_second_candidate_for_every_query() {
    // Arrange: a cluster with no `postgres` database, which is legal. initdb creates it and
    // nothing stops an administrator dropping it, which is why `template1` is the fallback.
    // The stand-in refuses `-d postgres` and answers everything else, so any query that
    // reconnects to the first candidate rather than the one that worked fails the read.
    let without_postgres = "\
template1,postgres,t,-1,t
orders,postgres,t,-1,f
";
    let body = format!(
        "case \"$*\" in\n\
         *\"-d postgres \"*) printf 'FATAL: no database \"postgres\"\\n' >&2; exit 2 ;;\n\
         esac\n\
         case \"$*\" in\n\
         *pg_read_all_settings*) cat <<'EOF'\n{LENS}EOF\n ;;\n\
         *pg_db_role_setting*) cat <<'EOF'\n{ROLE_SETTINGS}EOF\n ;;\n\
         *pg_file_settings*) cat <<'EOF'\n{FILE_SETTINGS}EOF\n ;;\n\
         *pg_control_system*) cat <<'EOF'\n{CONTROL}EOF\n ;;\n\
         *pg_hba_file_rules*) cat <<'EOF'\n{HBA_RULES}EOF\n ;;\n\
         *pg_available_extensions*) cat <<'EOF'\n{AVAILABLE_EXTENSIONS}EOF\n ;;\n\
         *pg_replication_slots*) cat <<'EOF'\n{REPLICATION_SLOTS}EOF\n ;;\n\
         *pg_settings*) cat <<'EOF'\n{SETTINGS}EOF\n ;;\n\
         *aclexplode*) cat <<'EOF'\n{DATABASE_GRANTS}EOF\n ;;\n\
         *pg_database*) cat <<'EOF'\n{without_postgres}EOF\n ;;\n\
         *pg_extension*) cat <<'EOF'\n{EXTENSIONS}EOF\n ;;\n\
         *pg_auth_members*) cat <<'EOF'\n{MEMBERSHIPS}EOF\n ;;\n\
         *pg_roles*) cat <<'EOF'\n{ROLES}EOF\n ;;\n\
         *) printf 'unexpected query: %s\\n' \"$*\" >&2; exit 3 ;;\n\
         esac"
    );

    // Act
    let clusters = read_with(
        "fallback",
        "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        &body,
    );

    // Assert: every catalogue is there, the grants included.
    let cluster = clusters.clusters().values().next().expect("one cluster");
    assert!(cluster.settings.is_some(), "settings");
    assert!(cluster.roles.is_some(), "roles");
    let databases = cluster.databases.as_ref().expect("databases");
    let orders = databases
        .databases()
        .iter()
        .find(|database| database.name.as_str() == "orders")
        .expect("the fixture lists orders");
    assert!(
        !orders
            .grants
            .as_ref()
            .expect("an ACL that is not the default")
            .is_empty(),
        "the grants read has to use the database that answered"
    );
}

#[test]
fn read_reports_which_cluster_could_not_be_answered_for() {
    // Arrange
    let refusing = PostgresqlClusters::reading_as(
        fake_inventory(
            "refused",
            "17  main    5432 online postgres /var/lib/pg /var/log/pg.log",
        ),
        fake_client("refused", "printf 'FATAL: no such database\\n' >&2\nexit 2"),
    );

    // Act
    let failure = refusing
        .read()
        .expect_err("the client refuses every database");

    // Assert: both candidate databases are named, because an operator told only that
    // `template1` failed cannot see that `postgres` was missing first.
    let reason = failure.to_string();
    assert!(reason.contains("17/main"), "got {reason:?}");
    assert!(reason.contains("postgres"), "got {reason:?}");
    assert!(reason.contains("template1"), "got {reason:?}");
}

#[test]
fn read_carries_every_cluster_the_register_lists() {
    // Act
    let clusters = read_with(
        "several",
        "17  main    5432 online postgres /var/lib/pg/17 /var/log/pg17.log\n\
         15  main    5433 down   postgres /var/lib/pg/15 /var/log/pg15.log",
        &answering_every_query(),
    );

    // Assert
    assert_eq!(
        keys_of(&Observation::from(&clusters)),
        vec!["15/main", "17/main"]
    );
}

#[test]
fn read_refuses_a_register_it_cannot_read() {
    // Arrange
    let broken = PostgresqlClusters::using(fake_inventory("broken", "17 main 5432"));

    // Act & Assert: a short row cannot be told apart from a cluster, so the register is
    // reported as unreadable rather than half-read.
    assert!(broken.read().is_err());
}

#[test]
fn using_names_the_register_it_will_run() {
    // Arrange
    let clusters = PostgresqlClusters::using(fake_inventory("named", ""));

    // Act & Assert
    assert_eq!(clusters.inventory().program(), "pg_lsclusters");
}

#[test]
fn a_cluster_id_refuses_a_half_it_cannot_render() {
    // Act & Assert: an empty half would render as `17/` or `/main`, which names no cluster
    // and would collide with the next one that lost the same half.
    assert!(ClusterId::new("", "main").is_err());
    assert!(ClusterId::new("17", "").is_err());
}

#[test]
fn read_carries_the_observed_half_from_the_pid_file() {
    // Arrange: a data directory with a postmaster.pid, read as the observed half. Line 4 is
    // the port the server is serving on, line 8 its status.
    let data_directory = scratch_tree("pgdata-observed", &[]);
    fs::write(
        data_directory.join("postmaster.pid"),
        "4242\n/var/lib/postgresql/17/main\n1700000000\n5599\n/var/run/postgresql\n*\n0\nready   \n",
    )
    .expect("a writable pid file");
    let listed = format!(
        "17 main 5432 online postgres {} /var/log/pg.log",
        data_directory.display()
    );

    // Act
    let clusters = read_with("observed", &listed, &answering_every_query());

    // Assert: the observed half carries the running port and status, kept apart from the
    // configured port the register printed.
    let cluster = clusters.clusters().values().next().expect("one cluster");
    let observed = cluster
        .observed
        .as_ref()
        .expect("a running cluster with a pid file");
    assert_eq!(observed.port, 5599);
    assert_eq!(observed.status, Some(PostmasterStatus::Ready));
    assert_eq!(cluster.port, Some(5432));
}

#[test]
fn read_connects_on_the_running_port_not_the_stale_configured_one() {
    // Arrange: the drift case. The port was edited in postgresql.conf and never reloaded, so
    // pg_lsclusters prints the new port (6000) while the server still serves the old one
    // (5599), which postmaster.pid records.
    let data_directory = scratch_tree("pgdata-drift", &[]);
    fs::write(
        data_directory.join("postmaster.pid"),
        "4242\n/data\n1700000000\n5599\n/run/postgresql\n*\n0\nready   \n",
    )
    .expect("a writable pid file");
    let listed = format!(
        "17 main 6000 online postgres {} /var/log/pg.log",
        data_directory.display()
    );
    let inner = answering_every_query();
    let only_on_running = format!(
        "case \"$*\" in \
         *\"-p 5599 \"*) {inner} ;; \
         *) printf 'refused on %s\\n' \"$*\" >&2; exit 2 ;; \
         esac"
    );

    // Act
    let clusters = read_with("drift", &listed, &only_on_running);

    // Assert: the cluster was read, which can only happen if the connection used the running
    // port 5599 rather than the stale configured 6000; the two stay apart in the document.
    let cluster = clusters.clusters().values().next().expect("one cluster");
    assert!(cluster.settings.is_some(), "connected on the running port");
    assert_eq!(cluster.observed.as_ref().expect("pid file").port, 5599);
    assert_eq!(cluster.port, Some(6000));
}
