//! The facet that describes the run: the clock reading it is stamped with, and the
//! effective decisions it resolved.

mod support;

use std::time::{Duration, UNIX_EPOCH};

use rastro::collectors::filesystem::WalkPolicy;
use rastro::collectors::{InvocationCollector, seconds_since_epoch};
use rastro_collector::{Collector, FacetName, FilesystemClaim, Observation, WalkedTree};
use rastro_fingerprint::Volatility;
use support::observation::{field, is_null, text};

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
    let collector =
        InvocationCollector::new(Observation::null(), Observation::from(&claimed), None);

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

#[test]
fn the_invocation_facet_names_the_binary_the_walk_left_out() {
    // Arrange
    let collector = InvocationCollector::new(
        Observation::null(),
        Observation::null(),
        Some("/var/tmp/rastro.zDeJEVKF".to_owned()),
    );

    // Act
    let reported = collector
        .collect()
        .expect("the clock on a test host is after 1970");

    // Assert: the walk omits the file it is running from, and an omission nothing accounts
    // for is the one thing this format does not do. Volatile, because `rastro-ssh` stages
    // the binary under a fresh `mktemp` name on every run, so the diffable view stays
    // byte-identical while the complete view says which file was left out.
    let observer = field(&reported, "observer");
    assert_eq!(text(&observer), "/var/tmp/rastro.zDeJEVKF");
    assert_eq!(observer.volatility(), Volatility::Volatile);
}

#[test]
fn the_invocation_facet_reports_no_observer_when_the_kernel_would_not_say() {
    // Arrange
    let collector = InvocationCollector::new(Observation::null(), Observation::null(), None);

    // Act
    let reported = collector
        .collect()
        .expect("the clock on a test host is after 1970");

    // Assert: null rather than absent, because the key is part of the facet's shape, and a
    // run that could not tell which file it is omits nothing from the walk either.
    assert!(is_null(&field(&reported, "observer")));
}
