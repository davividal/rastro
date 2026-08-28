//! Reading a cluster's control-file identity, without a cluster to read it from.
//!
//! `system_identifier` and `timeline_id` are not GUCs, so nothing in pg_settings produces
//! them. The first says which cluster this is (a restore from someone else's basebackup
//! shares it, a re-initdb changes it); the second increments on promotion.

use rastro::collectors::postgresql::PsqlControlData;

#[test]
fn parse_reads_the_system_identifier_and_timeline() {
    // Act
    let control = PsqlControlData::parse("7280634931331371930,3\n").expect("well formed");

    // Assert: the identifier is kept as text, because it is a 64-bit unsigned value a diff
    // compares rather than computes on.
    assert_eq!(control.system_identifier, "7280634931331371930");
    assert_eq!(control.timeline_id, 3);
}

#[test]
fn parse_refuses_an_empty_result() {
    // Act & Assert: a cluster always has a control file, so no row is a failed read.
    assert!(PsqlControlData::parse("").is_err());
}

#[test]
fn parse_refuses_more_than_one_row() {
    // Act & Assert: the control file is one row.
    assert!(PsqlControlData::parse("7280634931331371930,1\n7280634931331371931,2\n").is_err());
}

#[test]
fn parse_refuses_a_timeline_that_is_not_a_number() {
    // Act & Assert
    assert!(PsqlControlData::parse("7280634931331371930,latest\n").is_err());
}

#[test]
fn parse_refuses_an_empty_system_identifier() {
    // Act & Assert: an empty identifier names no cluster, so it is a failed read rather than
    // a cluster with no lineage.
    assert!(PsqlControlData::parse(",1\n").is_err());
}
