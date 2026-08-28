//! Reading a cluster's databases and who may do what to them.
//!
//! **The grants come from `aclexplode`, not from the text form of an `aclitem`.** A role
//! name may contain a space or an equals sign, and on the reference box a role called
//! `reporting team=x` rendered as `""reporting team=x""=C/postgres` inside a
//! space-joined array. Splitting that on whitespace and on the first `=` yields
//! `""reporting` as the grantee, so the text form is not parseable in general and
//! `aclexplode` is asked for rows instead.

mod support;

use rastro::collectors::postgresql::{Database, PsqlDatabaseGrants, PsqlDatabases};
use rastro_collector::Observation;
use support::observation::{boolean, field, is_null, items_of, keys_of, text};

/// The five columns the databases query asks for. The last is `datacl IS NULL`.
const DATABASES: &str = "\
postgres,postgres,t,-1,t
template0,postgres,f,-1,f
orders,postgres,t,-1,f
";

/// The five columns the grants query asks for, as `aclexplode` really answered on that box.
/// An empty grantee is `PUBLIC`, which is how the server spells grantee zero.
const GRANTS: &str = "\
orders,,CONNECT,f,postgres
orders,,TEMPORARY,f,postgres
orders,postgres,CONNECT,f,postgres
orders,postgres,CREATE,f,postgres
orders,migrator,CONNECT,f,postgres
orders,migrator,CREATE,t,postgres
";

fn databases(csv: &str) -> Vec<Database> {
    PsqlDatabases::parse(csv)
        .expect("this output is well formed")
        .databases()
        .to_vec()
}

fn rendered(csv: &str, database: &str) -> Observation {
    field(
        &Observation::from(&PsqlDatabases::parse(csv).expect("well formed")),
        database,
    )
}

fn grants_of(csv: &str, database: &str) -> Vec<Observation> {
    let parsed = PsqlDatabaseGrants::parse(csv).expect("well formed");
    let grants = parsed
        .of_database(database)
        .expect("the fixture grants on this database");

    grants.iter().map(Observation::from).collect()
}

#[test]
fn parse_reads_a_database_and_its_owner() {
    // Act
    let observed = rendered(DATABASES, "orders");

    // Assert: ownership carries `DROP DATABASE`, so who holds it is the first thing to
    // record about a database.
    assert_eq!(text(&field(&observed, "owner")), "postgres");
    assert!(boolean(&field(&observed, "allows_connections")));
}

#[test]
fn parse_reads_a_database_that_refuses_connections() {
    // Act
    let observed = rendered(DATABASES, "template0");

    // Assert: `template0` is kept unconnectable on purpose, so this is state rather than a
    // fault.
    assert!(!boolean(&field(&observed, "allows_connections")));
}

#[test]
fn parse_reads_an_unlimited_connection_limit_as_no_limit() {
    // Act
    let observed = rendered(DATABASES, "postgres");

    // Assert
    assert!(is_null(&field(&observed, "connection_limit")));
}

#[test]
fn parse_reads_a_null_acl_as_the_servers_own_defaults() {
    // Act
    let observed = rendered(DATABASES, "postgres");

    // Assert: null is not empty. Postgres leaves `datacl` null until somebody grants or
    // revokes, and null means the built-in defaults apply.
    assert!(is_null(&field(&observed, "grants")));
}

#[test]
fn parse_reads_an_acl_that_exists_and_grants_nothing() {
    // Act: `datacl IS NULL` is false, and the grants query returns no row for it.
    let observed = rendered("locked,postgres,t,-1,f\n", "locked");

    // Assert: an empty list, not null. Everything has been revoked from everybody, which is
    // the opposite state from the defaults applying, and the two are told apart by the
    // `datacl IS NULL` column rather than by an empty string that means both.
    assert!(items_of(&field(&observed, "grants")).is_empty());
}

#[test]
fn parse_reads_a_grant_made_to_public() {
    // Act
    let grants = grants_of(GRANTS, "orders");

    // Assert: grantee zero is `PUBLIC`, which is what every role's `CONNECT` actually rests
    // on.
    let public = grants
        .iter()
        .find(|grant| text(&field(grant, "grantee")) == "PUBLIC")
        .expect("a grant to PUBLIC");
    assert_eq!(
        keys_of(&field(public, "privileges")),
        vec!["CONNECT", "TEMPORARY"]
    );
}

#[test]
fn parse_gathers_one_grantees_privileges_into_one_grant() {
    // Act
    let grants = grants_of(GRANTS, "orders");

    // Assert: `aclexplode` returns a row per privilege, and a reader wants a grantee's
    // privileges together. The grant option belongs to the privilege, not the grantee, so
    // `CREATE` carries it and `CONNECT` does not.
    let migration = grants
        .iter()
        .find(|grant| text(&field(grant, "grantee")) == "migrator")
        .expect("a grant to migrator");
    let privileges = field(migration, "privileges");
    assert_eq!(keys_of(&privileges), vec!["CONNECT", "CREATE"]);
    assert!(boolean(&field(&field(&privileges, "CREATE"), "grantable")));
    assert!(!boolean(&field(
        &field(&privileges, "CONNECT"),
        "grantable"
    )));
    assert_eq!(text(&field(migration, "granted_by")), "postgres");
}

#[test]
fn parse_reads_a_grantee_whose_name_contains_a_delimiter() {
    // Arrange: legal, and the reason the text form of an aclitem is not parsed. psql quotes
    // the field for the comma, and the name reaches the document as the server spells it.
    let awkward = "orders,\"reporting team=x\",CREATE,f,postgres\n";

    // Act
    let grants = grants_of(awkward, "orders");

    // Assert
    assert_eq!(text(&field(&grants[0], "grantee")), "reporting team=x");
}

#[test]
fn parse_orders_the_grants_of_a_database() {
    // Arrange
    let unsorted = "\
orders,migrator,CREATE,t,postgres
orders,,CONNECT,f,postgres
orders,postgres,CREATE,f,postgres
";

    // Act
    let grants = grants_of(unsorted, "orders");

    // Assert: `PUBLIC` first, then by name, so two clusters holding the same grants render
    // the same bytes whatever order the server listed them in.
    let grantees: Vec<String> = grants
        .iter()
        .map(|grant| text(&field(grant, "grantee")))
        .collect();
    assert_eq!(grantees, vec!["PUBLIC", "migrator", "postgres"]);
}

#[test]
fn parse_orders_databases_by_name() {
    // Act
    let parsed = databases(DATABASES);

    // Assert
    let names: Vec<&str> = parsed
        .iter()
        .map(|database| database.name.as_str())
        .collect();
    assert_eq!(names, vec!["orders", "postgres", "template0"]);
}

#[test]
fn parse_refuses_a_privilege_it_does_not_know() {
    // Act
    let refused = PsqlDatabaseGrants::parse("orders,app,VACUUM,f,postgres\n");

    // Assert: a privilege this does not know means the server gained one at the database
    // level. Recording it as something else would put an invented grant in the document.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_the_same_privilege_for_one_grantee_twice() {
    // Arrange
    let contradiction = "\
orders,app,CREATE,f,postgres
orders,app,CREATE,t,postgres
";

    // Act
    let refused = PsqlDatabaseGrants::parse(contradiction);

    // Assert: one privilege is held once, so whether it may be passed on has one answer.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_two_rows_for_one_database() {
    // Arrange
    let contradiction = "\
orders,postgres,t,-1,f
orders,app,t,-1,f
";

    // Act
    let refused = PsqlDatabases::parse(contradiction);

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_output_with_no_databases_in_it() {
    // Act
    let refused = PsqlDatabases::parse("\n");

    // Assert: every cluster has `template1`, which cannot be dropped.
    assert!(refused.is_err());
}
