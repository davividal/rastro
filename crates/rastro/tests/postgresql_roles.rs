//! Reading a cluster's roles, without needing a cluster to read them from.
//!
//! The fixtures are shaped like the rows psql really printed on a PostgreSQL 17 cluster,
//! `ops_admin` and its superuser flag included: that box carries two superusers
//! besides `postgres`, which is the fact this read exists to surface.

use rastro::collectors::postgresql::{PasswordMethod, PsqlRoles, Role, RoleName};

/// The ten columns the collector's query asks for, in order. The tenth is how the password
/// is stored, derived in the query so no hash is ever read.
const ROLES: &str = "\
ops_admin,t,t,t,t,f,t,-1,,scram-sha-256
postgres,t,t,t,t,t,t,-1,,scram-sha-256
app,f,f,f,f,f,t,-1,,scram-sha-256
migrator,f,f,f,f,f,t,-1,,md5
";

fn parsed(csv: &str) -> Vec<Role> {
    PsqlRoles::parse(csv)
        .expect("this output is well formed")
        .roles()
        .to_vec()
}

fn named<'a>(roles: &'a [Role], name: &str) -> &'a Role {
    roles
        .iter()
        .find(|role| role.name == RoleName::new(name).expect("a legal name"))
        .expect("the fixture has this role")
}

#[test]
fn parse_reads_every_attribute_of_a_role() {
    // Act
    let roles = parsed(ROLES);

    // Assert
    let postgres = named(&roles, "postgres");
    assert!(postgres.superuser);
    assert!(postgres.creates_databases);
    assert!(postgres.creates_roles);
    assert!(postgres.replication);
    assert!(postgres.bypasses_row_level_security);
    assert!(postgres.can_login);
}

#[test]
fn parse_reads_a_role_that_holds_nothing() {
    // Act
    let roles = parsed(ROLES);

    // Assert: an application user declares nothing and is granted nothing, so every flag
    // being false is the state the tenancy model intends rather than a failed read.
    let application = named(&roles, "app");
    assert!(!application.superuser);
    assert!(!application.creates_databases);
    assert!(!application.creates_roles);
    assert!(!application.replication);
    assert!(!application.bypasses_row_level_security);
    assert!(application.can_login);
}

#[test]
fn parse_reads_an_unprivileged_superuser_apart_from_the_owner() {
    // Act
    let roles = parsed(ROLES);

    // Assert: a second superuser appearing is the security event a fingerprint exists to
    // catch, so the flag is recorded per role rather than summarised for the cluster.
    assert!(named(&roles, "ops_admin").superuser);
    assert!(!named(&roles, "ops_admin").bypasses_row_level_security);
}

#[test]
fn parse_reads_an_unlimited_connection_limit_as_no_limit() {
    // Act
    let roles = parsed(ROLES);

    // Assert: the server spells unlimited `-1`. Recording that as a number would put a
    // negative count of connections in the document and invite arithmetic on it.
    assert_eq!(named(&roles, "postgres").connection_limit, None);
}

#[test]
fn parse_reads_a_connection_limit_the_operator_set() {
    // Arrange
    let capped = "reporting,f,f,f,f,f,t,5,,scram-sha-256\n";

    // Act
    let roles = parsed(capped);

    // Assert
    assert_eq!(named(&roles, "reporting").connection_limit, Some(5));
}

#[test]
fn parse_reads_an_empty_expiry_as_no_expiry() {
    // Act
    let roles = parsed(ROLES);

    // Assert: psql renders a null and an empty string identically, and for this column
    // both mean the password never expires.
    assert_eq!(named(&roles, "postgres").valid_until, None);
}

#[test]
fn parse_reads_an_expiry_the_operator_set() {
    // Arrange: the timestamp is kept as the server printed it. Reformatting would invent a
    // timezone rendering rastro does not own.
    let expiring = "contractor,f,f,f,f,f,t,-1,2027-01-01 00:00:00+00,scram-sha-256\n";

    // Act
    let roles = parsed(expiring);

    // Assert
    assert_eq!(
        named(&roles, "contractor").valid_until.as_deref(),
        Some("2027-01-01 00:00:00+00")
    );
}

#[test]
fn parse_orders_roles_by_name() {
    // Arrange
    let unsorted = "\
app,f,f,f,f,f,t,-1,,scram-sha-256
ops_admin,t,t,t,t,f,t,-1,,scram-sha-256
postgres,t,t,t,t,t,t,-1,,scram-sha-256
";

    // Act
    let roles = parsed(unsorted);

    // Assert: list order is contractual, so it is decided here rather than inherited from
    // whatever order the server answered in.
    let names: Vec<&str> = roles.iter().map(|role| role.name.as_str()).collect();
    assert_eq!(names, vec!["app", "ops_admin", "postgres"]);
}

#[test]
fn parse_refuses_a_flag_that_is_not_a_boolean() {
    // Act
    let refused = PsqlRoles::parse("postgres,yes,f,f,f,f,t,-1,,scram-sha-256\n");

    // Assert: guessing that `yes` means true would put an invented privilege in the
    // document, and a privilege is the last thing to guess about.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_a_row_with_the_wrong_number_of_columns() {
    // Act
    let refused = PsqlRoles::parse("postgres,t,t,t\n");

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_two_rows_for_one_role() {
    // Arrange
    let contradiction = "\
postgres,t,t,t,t,t,t,-1,,scram-sha-256
postgres,f,f,f,f,f,f,-1,,md5
";

    // Act
    let refused = PsqlRoles::parse(contradiction);

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_output_with_no_roles_in_it() {
    // Act
    let refused = PsqlRoles::parse("\n");

    // Assert: a cluster always has at least the role that owns it, so nothing at all is a
    // failed read rather than a cluster with no roles.
    assert!(refused.is_err());
}

#[test]
fn parse_reads_how_a_password_is_stored() {
    // Act
    let roles = parsed(ROLES);

    // Assert: never the hash, only the scheme. An `md5` password on a modern cluster is
    // drift worth seeing, and seeing it needs no part of the secret.
    assert_eq!(
        named(&roles, "postgres").password_method,
        Some(PasswordMethod::ScramSha256)
    );
    assert_eq!(
        named(&roles, "migrator").password_method,
        Some(PasswordMethod::Md5)
    );
}

#[test]
fn parse_reads_a_role_with_no_password() {
    // Arrange: a group role nobody logs in as.
    let passwordless = "readers,f,f,f,f,f,f,-1,,\n";

    // Act
    let roles = parsed(passwordless);

    // Assert: having no password and having one are different states, and only one of them
    // lets a role log in with one.
    assert_eq!(named(&roles, "readers").password_method, None);
}

#[test]
fn parse_refuses_a_password_method_its_own_query_cannot_produce() {
    // Act
    let refused = PsqlRoles::parse("postgres,t,t,t,t,t,t,-1,,bcrypt\n");

    // Assert: the column is derived by this collector's own CASE, so a value it does not
    // recognise means the query and the parser have drifted apart.
    assert!(refused.is_err());
}
