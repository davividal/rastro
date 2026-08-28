//! Reading which roles a cluster's roles are members of.
//!
//! The fixture is shaped like a real cluster's: two monitoring accounts in `pg_monitor`, and
//! `developer` in `migrator`, which grants a developer everything the migration role owns.

mod support;

use rastro::collectors::postgresql::PsqlMemberships;
use rastro_collector::Observation;
use support::observation::{boolean, field, keys_of};

/// The three columns the collector's query asks for, in order.
const MEMBERSHIPS: &str = "\
metrics,pg_monitor,f
postgres-exp,pg_monitor,f
developer,migrator,f
";

fn parsed(csv: &str) -> Observation {
    Observation::from(&PsqlMemberships::parse(csv).expect("this output is well formed"))
}

#[test]
fn parse_reads_who_is_a_member_of_what() {
    // Act
    let memberships = parsed(MEMBERSHIPS);

    // Assert: keyed by the member, because that is the role an operator looks up when
    // asking what somebody can reach.
    assert_eq!(
        keys_of(&memberships),
        vec!["developer", "metrics", "postgres-exp"]
    );
    assert_eq!(keys_of(&field(&memberships, "developer")), vec!["migrator"]);
}

#[test]
fn parse_reads_a_membership_granted_without_the_admin_option() {
    // Act
    let memberships = parsed(MEMBERSHIPS);

    // Assert: without it the member cannot pass the membership on, which is the difference
    // between inheriting a role and being able to hand it out.
    let membership = field(&field(&memberships, "developer"), "migrator");
    assert!(!boolean(&field(&membership, "admin_option")));
}

#[test]
fn parse_reads_a_membership_granted_with_the_admin_option() {
    // Arrange
    let delegating = "developer,migrator,t\n";

    // Act
    let memberships = parsed(delegating);

    // Assert
    let membership = field(&field(&memberships, "developer"), "migrator");
    assert!(boolean(&field(&membership, "admin_option")));
}

#[test]
fn parse_gathers_every_role_one_member_holds() {
    // Arrange: a developer who inherits two migration users.
    let several = "\
developer,migrator,f
developer,reader,f
";

    // Act
    let memberships = parsed(several);

    // Assert: sorted within the member, so two clusters with the same grants render the
    // same bytes whatever order the server answered in.
    assert_eq!(
        keys_of(&field(&memberships, "developer")),
        vec!["migrator", "reader"]
    );
}

#[test]
fn parse_reads_a_cluster_with_no_memberships_at_all() {
    // Act
    let memberships = PsqlMemberships::parse("\n").expect("no memberships is an ordinary answer");

    // Assert: unlike settings and roles, none is a real state rather than a failed read. A
    // fresh cluster grants nothing to anybody.
    assert!(memberships.memberships().is_empty());
}

#[test]
fn parse_refuses_a_row_with_the_wrong_number_of_columns() {
    // Act
    let refused = PsqlMemberships::parse("developer,migrator\n");

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_the_same_membership_twice() {
    // Arrange
    let contradiction = "\
developer,migrator,f
developer,migrator,t
";

    // Act
    let refused = PsqlMemberships::parse(contradiction);

    // Assert: one grant cannot both carry the admin option and not carry it.
    assert!(refused.is_err());
}
