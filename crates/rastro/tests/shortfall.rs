//! What a run could not see, as the operator hears it on stderr.
//!
//! The document already records every failure. This is the part an operator reads without
//! opening it, so a run that came back partial never looks like one that came back whole.

use rastro::shortfall::Shortfall;
use rastro_fingerprint::{
    CollectorCategory, CollectorId, CollectorIdentity, CollectorVersion, Facet, FacetName,
    FacetOutcome, Fingerprint, Observation,
};

fn facet(name: &str, outcome: FacetOutcome) -> Facet {
    Facet::new(
        FacetName::new(name).expect("a legal facet name"),
        CollectorIdentity::new(
            CollectorId::new(name).expect("a legal collector id"),
            CollectorVersion::new("1").expect("a legal collector version"),
        ),
        CollectorCategory::State,
        outcome,
    )
}

fn fingerprint(facets: impl IntoIterator<Item = Facet>) -> Fingerprint {
    Fingerprint::from_facets(facets).expect("distinct facet names")
}

/// An item rastro was refused, marked the way a collector marks one.
fn refused() -> Observation {
    Observation::object([("error", Observation::text("Permission denied"))]).incomplete()
}

#[test]
fn a_run_that_read_everything_has_nothing_to_say() {
    // Arrange: absence is state, not a shortfall, and an `error` key the box reported about
    // itself is not rastro failing.
    let run = fingerprint([
        facet("mounts", FacetOutcome::ok(Observation::null())),
        facet("postgresql", FacetOutcome::Absent),
        facet(
            "firewall",
            FacetOutcome::ok(Observation::object([(
                "error",
                Observation::text("a fault the box reported"),
            )])),
        ),
    ]);

    // Act
    let shortfall = Shortfall::of(&run);

    // Assert
    assert!(shortfall.is_empty());
    assert_eq!(shortfall.messages(), Vec::<String>::new());
}

#[test]
fn a_failed_facet_is_named_with_its_reason() {
    // Arrange: what an unprivileged run gets from `accounts` and `cron`.
    let run = fingerprint([
        facet(
            "cron",
            FacetOutcome::error("could not list /var/spool/cron/crontabs: Permission denied"),
        ),
        facet(
            "accounts",
            FacetOutcome::error("could not read /etc/shadow: Permission denied"),
        ),
        facet("mounts", FacetOutcome::ok(Observation::null())),
    ]);

    // Act
    let messages = Shortfall::of(&run).messages();

    // Assert: sorted by facet, as the document is.
    assert_eq!(
        messages,
        ["2 facets could not be read:\n  \
          accounts: could not read /etc/shadow: Permission denied\n  \
          cron: could not list /var/spool/cron/crontabs: Permission denied"]
    );
}

#[test]
fn an_ok_facet_with_refused_items_is_counted_as_incomplete() {
    // Arrange: the rabbitmq case. The facet is `ok` and a node inside it could not be read,
    // which a status alone never shows.
    let run = fingerprint([
        facet(
            "rabbitmq",
            FacetOutcome::ok(Observation::object([("rabbit@box", refused())])),
        ),
        facet(
            "filesystem",
            FacetOutcome::ok(Observation::object([
                ("/root", refused()),
                ("/home/other", refused()),
                ("/etc", Observation::null()),
            ])),
        ),
    ]);

    // Act
    let messages = Shortfall::of(&run).messages();

    // Assert
    assert_eq!(
        messages,
        ["2 facets are incomplete:\n  \
          filesystem: 2 items could not be read\n  \
          rabbitmq: 1 item could not be read"]
    );
}

#[test]
fn a_run_short_both_ways_says_both() {
    // Arrange
    let run = fingerprint([
        facet(
            "accounts",
            FacetOutcome::error("could not read /etc/shadow: Permission denied"),
        ),
        facet(
            "filesystem",
            FacetOutcome::ok(Observation::object([("/root", refused())])),
        ),
    ]);

    // Act
    let messages = Shortfall::of(&run).messages();

    // Assert: failed facets first, since they are the larger loss.
    assert_eq!(
        messages,
        [
            "1 facet could not be read:\n  accounts: could not read /etc/shadow: Permission denied",
            "1 facet is incomplete:\n  filesystem: 1 item could not be read",
        ]
    );
}
