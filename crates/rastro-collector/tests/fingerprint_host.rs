use std::cell::Cell;
use std::rc::Rc;

use rastro_collector::fingerprint_host;
use rastro_collector::{CollectionError, Collector, Presence};
use rastro_fingerprint::FingerprintError;
use rastro_fingerprint::Observation;
use rastro_fingerprint::{CollectorCategory, CollectorId, CollectorIdentity, CollectorVersion};
use rastro_fingerprint::{FacetName, FacetOutcome};

/// A collector whose answers the test dictates, and which counts how often it
/// was asked to collect.
struct StubCollector {
    name: FacetName,
    identity: CollectorIdentity,
    presence: Presence,
    collected: Result<Observation, CollectionError>,
    collect_calls: Rc<Cell<usize>>,
}

impl StubCollector {
    fn new(
        name: &str,
        presence: Presence,
        collected: Result<Observation, CollectionError>,
    ) -> Self {
        Self::counting(name, presence, collected, Rc::new(Cell::new(0)))
    }

    fn counting(
        name: &str,
        presence: Presence,
        collected: Result<Observation, CollectionError>,
        collect_calls: Rc<Cell<usize>>,
    ) -> Self {
        Self {
            name: FacetName::new(name).expect("test facet names should be legal"),
            identity: CollectorIdentity::new(
                CollectorId::new(name).expect("test collector ids should be legal"),
                CollectorVersion::new("1").expect("test collector versions should be legal"),
            ),
            presence,
            collected,
            collect_calls,
        }
    }
}

impl Collector for StubCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    fn presence(&self) -> Presence {
        self.presence.clone()
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        self.collect_calls.set(self.collect_calls.get() + 1);
        self.collected.clone()
    }
}

fn present(name: &str) -> StubCollector {
    StubCollector::new(
        name,
        Presence::Present,
        Ok(Observation::text("collected value")),
    )
}

fn single_outcome(collector: StubCollector) -> FacetOutcome {
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(collector)];
    let fingerprint = fingerprint_host::run(&collectors).expect("one collector is consistent");
    fingerprint.facets()[0].outcome.clone()
}

#[test]
fn a_present_collector_that_succeeds_becomes_an_ok_facet() {
    // Act
    let outcome = single_outcome(present("mounts"));

    // Assert
    assert_eq!(
        outcome,
        FacetOutcome::ok(Observation::text("collected value"))
    );
}

#[test]
fn an_absent_collector_becomes_an_absent_facet() {
    // Act
    let outcome = single_outcome(StubCollector::new(
        "nginx",
        Presence::Absent,
        Ok(Observation::null()),
    ));

    // Assert
    assert_eq!(outcome, FacetOutcome::Absent);
}

#[test]
fn an_absent_collector_is_never_asked_to_collect() {
    // Arrange
    let calls = Rc::new(Cell::new(0));
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::counting(
        "nginx",
        Presence::Absent,
        Ok(Observation::null()),
        Rc::clone(&calls),
    ))];

    // Act
    fingerprint_host::run(&collectors).expect("one collector is consistent");

    // Assert
    assert_eq!(calls.get(), 0, "an absent subject must not be read");
}

#[test]
fn a_present_collector_is_asked_to_collect_once() {
    // Arrange
    let calls = Rc::new(Cell::new(0));
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::counting(
        "mounts",
        Presence::Present,
        Ok(Observation::null()),
        Rc::clone(&calls),
    ))];

    // Act
    fingerprint_host::run(&collectors).expect("one collector is consistent");

    // Assert
    assert_eq!(calls.get(), 1);
}

#[test]
fn an_undetermined_presence_becomes_an_error_facet_carrying_the_reason() {
    // Act
    let outcome = single_outcome(StubCollector::new(
        "postgres",
        Presence::Undetermined {
            reason: "pg_isready timed out after 5s".to_owned(),
        },
        Ok(Observation::null()),
    ));

    // Assert: an unanswerable check must never be recorded as absence.
    assert_eq!(
        outcome,
        FacetOutcome::error("pg_isready timed out after 5s")
    );
}

#[test]
fn an_undetermined_presence_is_never_asked_to_collect() {
    // Arrange
    let calls = Rc::new(Cell::new(0));
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::counting(
        "postgres",
        Presence::Undetermined {
            reason: "pg_isready timed out after 5s".to_owned(),
        },
        Ok(Observation::null()),
        Rc::clone(&calls),
    ))];

    // Act
    fingerprint_host::run(&collectors).expect("one collector is consistent");

    // Assert
    assert_eq!(calls.get(), 0);
}

#[test]
fn a_present_collector_that_fails_becomes_an_error_facet_carrying_the_message() {
    // Act
    let outcome = single_outcome(StubCollector::new(
        "nftables",
        Presence::Present,
        Err(CollectionError::new("nft exited 1: permission denied")),
    ));

    // Assert
    assert_eq!(
        outcome,
        FacetOutcome::error("nft exited 1: permission denied")
    );
}

#[test]
fn run_orders_the_facets_by_name() {
    // Arrange
    let collectors: Vec<Box<dyn Collector>> = vec![
        Box::new(present("processes")),
        Box::new(present("fs")),
        Box::new(present("mounts")),
    ];

    // Act
    let fingerprint = fingerprint_host::run(&collectors).expect("distinct names are consistent");

    // Assert
    let names: Vec<&str> = fingerprint
        .facets()
        .iter()
        .map(|facet| facet.name.as_str())
        .collect();
    assert_eq!(names, ["fs", "mounts", "processes"]);
}

#[test]
fn two_collectors_claiming_the_same_facet_name_fail_the_run() {
    // Arrange
    let collectors: Vec<Box<dyn Collector>> =
        vec![Box::new(present("mounts")), Box::new(present("mounts"))];

    // Act
    let result = fingerprint_host::run(&collectors);

    // Assert
    assert_eq!(
        result,
        Err(FingerprintError::DuplicateFacetName {
            name: "mounts".to_owned()
        })
    );
}
