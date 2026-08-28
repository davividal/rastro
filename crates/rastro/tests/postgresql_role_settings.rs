//! Reading a cluster's `ALTER ROLE`/`ALTER DATABASE` defaults, without a cluster to read
//! them from.
//!
//! These are the overrides `pg_settings` silently folds into one session's map. The fixtures
//! carry all three scopes the catalog records, because which scope an override has is the
//! whole of what tells a diff a value is global from one that is not.

use rastro::collectors::postgresql::{
    ClusterRoleSettings, DatabaseName, PsqlRoleSettings, RoleName, RoleSetting, SettingName,
    SettingValue,
};

/// The three columns the collector's query asks for: an `ALTER ROLE`, an `ALTER DATABASE`,
/// and an `ALTER ROLE ... IN DATABASE`, deliberately given out of sorted order.
const OVERRIDES: &str = "\
orders,migrator,search_path=public
,app,work_mem=256MB
orders,,statement_timeout=5000
";

fn parsed(csv: &str) -> Vec<RoleSetting> {
    PsqlRoleSettings::parse(csv)
        .expect("this output is well formed")
        .settings()
        .to_vec()
}

fn named<'a>(settings: &'a [RoleSetting], name: &str) -> &'a RoleSetting {
    settings
        .iter()
        .find(|setting| setting.name == SettingName::new(name).expect("a legal name"))
        .expect("the fixture has this override")
}

#[test]
fn parse_reads_a_role_scoped_override() {
    // Act
    let settings = parsed(OVERRIDES);

    // Assert: `ALTER ROLE app SET work_mem` applies to that role in every database, so the
    // database is absent and the role is not.
    let override_ = named(&settings, "work_mem");
    assert_eq!(override_.database, None);
    assert_eq!(
        override_.role,
        Some(RoleName::new("app").expect("legal"))
    );
    assert_eq!(override_.value, SettingValue::new("256MB"));
}

#[test]
fn parse_reads_a_database_scoped_override() {
    // Act
    let settings = parsed(OVERRIDES);

    // Assert: `ALTER DATABASE orders SET statement_timeout` applies to every role in
    // that database, so the role is absent and the database is not.
    let override_ = named(&settings, "statement_timeout");
    assert_eq!(
        override_.database,
        Some(DatabaseName::new("orders").expect("legal"))
    );
    assert_eq!(override_.role, None);
}

#[test]
fn parse_reads_an_override_scoped_to_a_role_in_a_database() {
    // Act
    let settings = parsed(OVERRIDES);

    // Assert: `ALTER ROLE migrator IN DATABASE orders SET` carries both oids, so
    // both scopes are present.
    let override_ = named(&settings, "search_path");
    assert_eq!(
        override_.database,
        Some(DatabaseName::new("orders").expect("legal"))
    );
    assert_eq!(
        override_.role,
        Some(RoleName::new("migrator").expect("legal"))
    );
}

#[test]
fn parse_splits_a_value_on_the_first_equals_only() {
    // Arrange: a `setconfig` element whose value carries more `=` than the assignment does.
    let conninfo = ",,primary_conninfo=host=primary password=secret\n";

    // Act
    let settings = parsed(conninfo);

    // Assert: the name stops at the first `=`; everything after it is the value, verbatim.
    assert_eq!(
        settings[0].name,
        SettingName::new("primary_conninfo").expect("legal")
    );
    assert_eq!(
        settings[0].value,
        SettingValue::new("host=primary password=secret")
    );
}

#[test]
fn parse_reads_a_cluster_with_no_overrides_as_an_empty_set() {
    // Act & Assert: unlike the effective settings, no `ALTER ROLE`/`ALTER DATABASE` default
    // is the common case, so nothing is a real state rather than a failed read.
    assert!(
        PsqlRoleSettings::parse("")
            .expect("no overrides is well formed")
            .settings()
            .is_empty()
    );
}

#[test]
fn new_refuses_one_override_named_twice_for_the_same_scope() {
    // Arrange: `setconfig` holds a name once per scope, so a repeat means two reads were
    // spliced or the catalog was misread.
    let same_scope = |value: &str| RoleSetting {
        database: None,
        role: Some(RoleName::new("app").expect("legal")),
        name: SettingName::new("work_mem").expect("legal"),
        value: SettingValue::new(value),
    };

    // Act & Assert
    assert!(ClusterRoleSettings::new(vec![same_scope("64MB"), same_scope("128MB")]).is_err());
}

#[test]
fn parse_orders_overrides_by_scope_then_name() {
    // Act
    let settings = parsed(OVERRIDES);

    // Assert: list order is a contract rastro keeps, so a defined order is asserted rather
    // than the arrival order the server happened to use. An absent scope sorts before a
    // present one, so the role-only override leads.
    let order: Vec<&str> = settings
        .iter()
        .map(|setting| setting.name.as_str())
        .collect();
    assert_eq!(order, vec!["work_mem", "statement_timeout", "search_path"]);
}
