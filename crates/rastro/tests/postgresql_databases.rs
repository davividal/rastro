//! Reading a cluster's databases and who may do what to them.
//!
//! **The grants come from `aclexplode`, not from the text form of an `aclitem`.** A role
//! name may contain a space or an equals sign, and on the reference box a role called
//! `reporting team=x` rendered as `""reporting team=x""=C/postgres` inside a
//! space-joined array. Splitting that on whitespace and on the first `=` yields
//! `""reporting` as the grantee, so the text form is not parseable in general and
//! `aclexplode` is asked for rows instead.
//!
//! **The grants of a database are keyed by grantee**, and the tests below pin that because it
//! is what makes a diff of two fingerprints readable. A grantee gaining an explicit entry
//! shifts every later element of a list along, so a plain diff reports the shift as though
//! every grant after it had changed hands, and a revoke buried among those lines is the one
//! thing a reader is looking for. The grantor stays a field: keyed on it too, an
//! `ALTER DATABASE … OWNER` renames every key at once and the revoke disappears entirely.

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

/// The grants of one database, as the document holds them.
fn grants_of(csv: &str, database: &str) -> Observation {
    let parsed = PsqlDatabaseGrants::parse(csv).expect("well formed");
    let grants = parsed
        .of_database(database)
        .expect("the fixture grants on this database");

    Observation::from(&grants)
}

/// The one grant a grantee holds, for the ordinary ACL where nobody granted twice.
fn only_grant(grants: &Observation, grantee: &str) -> Observation {
    let held = items_of(&field(grants, grantee));
    assert_eq!(held.len(), 1, "{grantee:?} holds one grant in this fixture");

    held[0].clone()
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

    // Assert: an object with no keys, not null. Everything has been revoked from everybody,
    // which is the opposite state from the defaults applying, and the two are told apart by
    // the `datacl IS NULL` column rather than by an empty string that means both.
    assert!(keys_of(&field(&observed, "grants")).is_empty());
}

#[test]
fn parse_keys_the_grants_of_a_database_by_grantee() {
    // Act
    let grants = grants_of(GRANTS, "orders");

    // Assert: `PUBLIC` first, then by name, so two clusters holding the same grants render
    // the same bytes whatever order the server listed them in.
    assert_eq!(keys_of(&grants), vec!["PUBLIC", "migrator", "postgres"]);
}

#[test]
fn parse_reads_a_grant_made_to_public() {
    // Act
    let grants = grants_of(GRANTS, "orders");

    // Assert: grantee zero is `PUBLIC`, which is what every role's `CONNECT` actually rests
    // on.
    let public = only_grant(&grants, "PUBLIC");
    assert_eq!(text(&field(&public, "granted_by")), "postgres");
    assert_eq!(
        keys_of(&field(&public, "privileges")),
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
    let privileges = field(&only_grant(&grants, "migrator"), "privileges");
    assert_eq!(keys_of(&privileges), vec!["CONNECT", "CREATE"]);
    assert!(boolean(&field(&field(&privileges, "CREATE"), "grantable")));
    assert!(!boolean(&field(
        &field(&privileges, "CONNECT"),
        "grantable"
    )));
}

#[test]
fn parse_orders_one_grantees_grants_by_the_role_that_made_them() {
    // Arrange: legal, and the reason a grantee alone cannot be the key. One role holds
    // `CONNECT` from `postgres` and `CREATE` from `migrator`, and a `REVOKE` has to name the
    // grantor to take either away.
    let two_grantors = "\
orders,app,CONNECT,f,postgres
orders,app,CREATE,f,migrator
";

    // Act
    let grants = grants_of(two_grantors, "orders");

    // Assert: one key, holding both grants, ordered by the role that made them.
    let app = items_of(&field(&grants, "app"));
    assert_eq!(app.len(), 2);
    assert_eq!(text(&field(&app[0], "granted_by")), "migrator");
    assert_eq!(keys_of(&field(&app[0], "privileges")), vec!["CREATE"]);
    assert_eq!(text(&field(&app[1], "granted_by")), "postgres");
    assert_eq!(keys_of(&field(&app[1], "privileges")), vec!["CONNECT"]);
}

#[test]
fn parse_reads_a_grantee_whose_name_contains_a_delimiter() {
    // Arrange: legal, and the reason the text form of an aclitem is not parsed. psql quotes
    // the field for the comma, and the name reaches the document as the server spells it.
    let awkward = "orders,\"reporting team=x\",CREATE,f,postgres\n";

    // Act
    let grants = grants_of(awkward, "orders");

    // Assert
    assert_eq!(keys_of(&grants), vec!["reporting team=x"]);
}

#[test]
fn a_grantee_appearing_leaves_every_other_grant_rendering_identically() {
    // Arrange: one `ALTER DATABASE … OWNER`, which gives the new owner an explicit entry the
    // ACL did not carry before. This is the case the keyed shape exists for.
    let before = "\
orders,,CONNECT,f,postgres
orders,migrator,CREATE,t,postgres
";
    let after = "\
orders,,CONNECT,f,postgres
orders,dbowner,CREATE,f,postgres
orders,migrator,CREATE,t,postgres
";

    // Act
    let before = grants_of(before, "orders");
    let after = grants_of(after, "orders");

    // Assert: one key added and nothing else touched. Listed positionally, `migrator` moved
    // from index 1 to index 2 and a diff reported it as a grant changing hands.
    assert_eq!(keys_of(&before), vec!["PUBLIC", "migrator"]);
    assert_eq!(keys_of(&after), vec!["PUBLIC", "dbowner", "migrator"]);
    assert_eq!(field(&before, "PUBLIC"), field(&after, "PUBLIC"));
    assert_eq!(field(&before, "migrator"), field(&after, "migrator"));
}

#[test]
fn a_revoked_privilege_is_the_only_thing_that_moves() {
    // Arrange: `ALTER DATABASE … OWNER` also takes the implicit owner rights off the old
    // owner, which shows as privileges leaving one grant.
    let before = "orders,migrator,CONNECT,t,postgres\norders,migrator,CREATE,t,postgres\n";
    let after = "orders,migrator,CREATE,t,postgres\n";

    // Act
    let before = grants_of(before, "orders");
    let after = grants_of(after, "orders");

    // Assert: the revoke is one key leaving one object, at a path naming the grantee it was
    // taken from.
    assert_eq!(
        keys_of(&field(&only_grant(&before, "migrator"), "privileges")),
        vec!["CONNECT", "CREATE"]
    );
    assert_eq!(
        keys_of(&field(&only_grant(&after, "migrator"), "privileges")),
        vec!["CREATE"]
    );
}

#[test]
fn a_change_of_owner_leaves_each_grantee_one_field_rewritten() {
    // Arrange: the grantor is a field rather than half the key, and this is why. `ALTER
    // DATABASE … OWNER` rewrites `granted_by` on every entry at once. Keyed on the grantor,
    // that renames every key, each grant reads as one removed and one added, and a diff never
    // descends far enough to report the privileges the same statement revoked.
    let before = "orders,app,CONNECT,f,migrator\norders,app,CREATE,f,migrator\n";
    let after = "orders,app,CREATE,f,postgres\n";

    // Act
    let before = only_grant(&grants_of(before, "orders"), "app");
    let after = only_grant(&grants_of(after, "orders"), "app");

    // Assert: the grantee keeps its key, so the rewrite and the revoke are both visible under
    // it rather than the second being swallowed by the first.
    assert_eq!(text(&field(&before, "granted_by")), "migrator");
    assert_eq!(text(&field(&after, "granted_by")), "postgres");
    assert_eq!(
        keys_of(&field(&before, "privileges")),
        vec!["CONNECT", "CREATE"]
    );
    assert_eq!(keys_of(&field(&after, "privileges")), vec!["CREATE"]);
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
