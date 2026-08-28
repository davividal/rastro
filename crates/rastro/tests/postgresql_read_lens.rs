//! Reading the lens a cluster's settings were seen through, without a cluster to read from.
//!
//! `pg_settings` is one session's view, so the role and database that answered decide which
//! `ALTER ... SET` defaults it folds in and whether it drops the `GUC_SUPERUSER_ONLY` rows.
//! The lens records that session so a reader of a diff can tell which distortion applies.

use rastro::collectors::postgresql::{PsqlReadLens, ReadLens};

fn parsed(csv: &str) -> ReadLens {
    PsqlReadLens::parse(csv).expect("this output is well formed")
}

#[test]
fn parse_reads_the_role_and_database_that_answered() {
    // Act
    let lens = parsed("app,orders,f,f\n");

    // Assert
    assert_eq!(lens.role.as_str(), "app");
    assert_eq!(lens.database.as_str(), "orders");
}

#[test]
fn a_superuser_sees_every_setting() {
    // Act
    let lens = parsed("postgres,postgres,t,f\n");

    // Assert: a superuser is not subject to the GUC_SUPERUSER_ONLY filter, so the map is
    // complete whether or not the role also holds pg_read_all_settings.
    assert!(lens.is_superuser);
    assert!(lens.sees_all_settings());
}

#[test]
fn a_non_superuser_with_the_grant_sees_every_setting() {
    // Act
    let lens = parsed("monitor,postgres,f,t\n");

    // Assert: pg_read_all_settings is the one grant that lifts the filter for a non-superuser.
    assert!(!lens.is_superuser);
    assert!(lens.reads_all_settings);
    assert!(lens.sees_all_settings());
}

#[test]
fn a_non_superuser_without_the_grant_loses_the_superuser_only_settings() {
    // Act
    let lens = parsed("app,orders,f,f\n");

    // Assert: the 21 GUC_SUPERUSER_ONLY rows are dropped with no word from the server, which
    // is what the cluster records as an incomplete settings map.
    assert!(!lens.sees_all_settings());
}

#[test]
fn parse_refuses_an_empty_result() {
    // Act & Assert: a live connection always has an identity, so no row is a failed read.
    assert!(PsqlReadLens::parse("").is_err());
}

#[test]
fn parse_refuses_more_than_one_row() {
    // Act & Assert: a session's own identity is one row, so two means the query handed in was
    // not the lens query.
    assert!(PsqlReadLens::parse("app,one,f,f\nother,two,t,t\n").is_err());
}
