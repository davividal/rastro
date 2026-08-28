//! The facet that describes the run: the clock reading it is stamped with, and the
//! effective decisions it resolved.

mod support;

use std::time::{Duration, UNIX_EPOCH};

use rastro::collectors::filesystem::WalkPolicy;
use rastro::collectors::{InvocationCollector, seconds_since_epoch};
use rastro_collector::{Collector, FacetName, FilesystemClaim, Observation, WalkedTree};
use support::observation::{field, text};

#[test]
fn seconds_since_epoch_counts_from_1970() {
    // Arrange
    let reading = UNIX_EPOCH + Duration::from_secs(1_786_632_455);

    // Act
    let seconds = seconds_since_epoch(reading).expect("a reading after 1970 is representable");

    // Assert
    assert_eq!(seconds, 1_786_632_455);
}

#[test]
fn seconds_since_epoch_refuses_a_clock_set_before_1970() {
    // Arrange: a box with a dead RTC really does come up in 1969.
    let reading = UNIX_EPOCH - Duration::from_secs(1);

    // Act
    let result = seconds_since_epoch(reading);

    // Assert: recorded as a failed facet, never as a wrapped-around timestamp.
    let failure = result.expect_err("a pre-1970 clock cannot be timestamped");
    assert!(
        failure.to_string().contains("before 1970"),
        "the operator needs to know which clock is wrong, got: {failure}"
    );
}

#[test]
fn seconds_since_epoch_accepts_the_epoch_itself() {
    // Act & Assert
    assert_eq!(seconds_since_epoch(UNIX_EPOCH), Ok(0));
}

#[test]
fn the_invocation_facet_carries_the_effective_walk_table() {
    // Arrange: the table as a run would resolve it, with one collector's claim folded in.
    let claimed = WalkPolicy::built_in()
        .claimed(
            &FacetName::new("postgresql").expect("a legal facet name"),
            &[FilesystemClaim::sealed(
                WalkedTree::new("/var/lib/postgresql/17/main").expect("a legal tree"),
            )],
        )
        .expect("a tree no shipped rule names");
    let collector = InvocationCollector::new(Observation::null(), Observation::from(&claimed));

    // Act
    let reported = collector
        .collect()
        .expect("the clock on a test host is after 1970");

    // Assert: this is the only place a reader can tell a policy decision from a gap, so the
    // facet that describes the run is where it has to be, not the one it describes.
    let table = field(&reported, "walk_policy");
    let cluster = field(&table, "/var/lib/postgresql/17/main");
    assert_eq!(text(&field(&cluster, "reading")), "sealed");
    assert_eq!(text(&field(&cluster, "claimed_by")), "postgresql");
}
