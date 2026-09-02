//! Reading a cluster's roles, without needing a cluster to read them from.
//!
//! The fixtures are shaped like the rows psql really printed on a PostgreSQL 17 cluster,
//! `ops_admin` and its superuser flag included: that box carries two superusers
//! besides `postgres`, which is the fact this read exists to surface.

use rastro::collectors::postgresql::{PasswordMethod, PsqlRoles, Role, RoleName};

/// The eleven columns the collector's query asks for, in order. The last two are derived in
/// the query from `pg_authid.rolpassword`: how the password is stored, and the sha256 of the
/// stored verifier. Neither is the verifier, so no secret is ever read. `migrator` carries an
/// md5 password and so an empty digest column, which is what the query prints for every
/// scheme but SCRAM.
const ROLES: &str = "\
ops_admin,t,t,t,t,f,t,-1,,scram-sha-256,b6f5ee30bf59607fcb1a5029c343bb875f4f197e9752d636d3b2a933dde9b192
postgres,t,t,t,t,t,t,-1,,scram-sha-256,122b86ea371aaa1f7176c6eb880cb9ac57b21a8f10ad502b321ddb42688067db
app,f,f,f,f,f,t,-1,,scram-sha-256,84757bf01cf42c7fdb177d4f37e255bb0dfa6ce3c92ecf71345002fdcaf8813a
migrator,f,f,f,f,f,t,-1,,md5,
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
    let capped = "reporting,f,f,f,f,f,t,5,,scram-sha-256,b6f5ee30bf59607fcb1a5029c343bb875f4f197e9752d636d3b2a933dde9b192\n";

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
    let expiring = "contractor,f,f,f,f,f,t,-1,2027-01-01 00:00:00+00,scram-sha-256,122b86ea371aaa1f7176c6eb880cb9ac57b21a8f10ad502b321ddb42688067db\n";

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
app,f,f,f,f,f,t,-1,,scram-sha-256,84757bf01cf42c7fdb177d4f37e255bb0dfa6ce3c92ecf71345002fdcaf8813a
ops_admin,t,t,t,t,f,t,-1,,scram-sha-256,b6f5ee30bf59607fcb1a5029c343bb875f4f197e9752d636d3b2a933dde9b192
postgres,t,t,t,t,t,t,-1,,scram-sha-256,122b86ea371aaa1f7176c6eb880cb9ac57b21a8f10ad502b321ddb42688067db
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
    let refused = PsqlRoles::parse(
        "postgres,yes,f,f,f,f,t,-1,,scram-sha-256,\
         122b86ea371aaa1f7176c6eb880cb9ac57b21a8f10ad502b321ddb42688067db\n",
    );

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
postgres,t,t,t,t,t,t,-1,,scram-sha-256,122b86ea371aaa1f7176c6eb880cb9ac57b21a8f10ad502b321ddb42688067db
postgres,f,f,f,f,f,f,-1,,md5,122b86ea371aaa1f7176c6eb880cb9ac57b21a8f10ad502b321ddb42688067db
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
    let passwordless = "readers,f,f,f,f,f,f,-1,,,\n";

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

#[test]
fn parse_reads_a_password_digest() {
    // Act
    let roles = parsed(ROLES);

    // Assert: the value the document carries is XXH3-64 over the sha256 the server printed,
    // rendered the way the filesystem walker renders an entry digest.
    let digest = named(&roles, "app")
        .password_digest
        .expect("the fixture role has a password");
    assert_eq!(digest.as_str().len(), 16);
    assert!(
        digest
            .as_str()
            .bytes()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    );
}

#[test]
fn parse_reads_no_digest_for_a_role_with_no_password() {
    // Arrange: a group role nobody logs in as. The query prints an empty column for it,
    // because there is no verifier to hash rather than a verifier that hashed to nothing.
    let passwordless = "readers,f,f,f,f,f,f,-1,,,\n";

    // Act
    let roles = parsed(passwordless);

    // Assert
    assert_eq!(named(&roles, "readers").password_digest, None);
}

#[test]
fn a_rotated_password_gives_a_different_digest() {
    // Arrange: the same role either side of an `ALTER ROLE ... PASSWORD`. PostgreSQL
    // re-salts on every set, so the verifier differs even where the password does not, and
    // this is the whole change the digest exists to make visible.
    let before = "app,f,f,f,f,f,t,-1,,scram-sha-256,\
                  84757bf01cf42c7fdb177d4f37e255bb0dfa6ce3c92ecf71345002fdcaf8813a\n";
    let after = "app,f,f,f,f,f,t,-1,,scram-sha-256,\
                 64acc59a019e74b5794bc6bc005ce55a48dcb7f1ca9dda37cc2b3af72f9c47a9\n";

    // Act
    let first = parsed(before);
    let second = parsed(after);

    // Assert
    assert_ne!(
        named(&first, "app").password_digest,
        named(&second, "app").password_digest
    );
}

#[test]
fn an_untouched_password_gives_the_same_digest() {
    // Act: two reads of one unchanged cluster, which is the case the output format's
    // byte-identity promise rests on.
    let first = parsed(ROLES);
    let second = parsed(ROLES);

    // Assert
    assert_eq!(
        named(&first, "app").password_digest,
        named(&second, "app").password_digest
    );
}

#[test]
fn a_password_digest_tells_two_roles_apart() {
    // Act
    let roles = parsed(ROLES);

    // Assert: each role's verifier carries its own salt, so two roles sharing a password
    // still differ here. A digest equal across two roles would mean the same verifier was
    // written to both, which only a pushed pre-computed verifier can do.
    assert_ne!(
        named(&roles, "app").password_digest,
        named(&roles, "postgres").password_digest
    );
}

#[test]
fn parse_refuses_a_digest_its_own_query_cannot_produce() {
    // Arrange: the query prints sixty-four hex characters or nothing at all.
    let truncated = "app,f,f,f,f,f,t,-1,,scram-sha-256,84757bf0\n";

    // Act
    let refused = PsqlRoles::parse(truncated);

    // Assert: hashing a short column anyway would put a digest in the document that no
    // later run can reproduce, and a fingerprint that cannot be compared is worse than a
    // read that failed.
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_a_digest_that_is_not_hexadecimal() {
    // Arrange
    let corrupt = format!("app,f,f,f,f,f,t,-1,,scram-sha-256,{}\n", "z".repeat(64));

    // Act
    let refused = PsqlRoles::parse(&corrupt);

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_a_digest_in_the_other_case() {
    // Arrange: `encode` prints lowercase. The same sixty-four characters uppercased are the
    // same sha256 and would digest differently.
    let uppercased = "app,f,f,f,f,f,t,-1,,scram-sha-256,\
                      84757BF01CF42C7FDB177D4F37E255BB0DFA6CE3C92ECF71345002FDCAF8813A\n";

    // Act
    let refused = PsqlRoles::parse(uppercased);

    // Assert: accepting it would report a password change on a cluster where nothing moved,
    // which is the one failure a fingerprint must not invent.
    assert!(refused.is_err());
}

#[test]
fn parse_reads_no_digest_for_an_md5_password() {
    // Act
    let roles = parsed(ROLES);

    // Assert: the query prints an empty digest column for md5, because an md5 verifier is
    // `md5(password || rolname)` with no random salt, and the role name is this facet's own
    // key. Digesting it would put an offline password oracle in the document. The method is
    // still recorded, which is what makes the absence readable.
    let migrator = named(&roles, "migrator");
    assert_eq!(migrator.password_method, Some(PasswordMethod::Md5));
    assert_eq!(migrator.password_digest, None);
}

#[test]
fn parse_reads_no_digest_for_a_scheme_it_does_not_recognise() {
    // Arrange: the query tests for SCRAM rather than excluding md5, so a scheme a later
    // PostgreSQL adds arrives with an empty digest column until somebody has checked how it
    // is salted.
    let unknown = "future,f,f,f,f,f,t,-1,,unrecognised,\n";

    // Act
    let roles = parsed(unknown);

    // Assert
    let role = named(&roles, "future");
    assert_eq!(role.password_method, Some(PasswordMethod::Unrecognised));
    assert_eq!(role.password_digest, None);
}
