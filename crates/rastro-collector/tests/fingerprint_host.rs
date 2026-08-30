use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rastro_collector::fingerprint_host;
use rastro_collector::{CollectionError, Collector, Concurrency, Presence};
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
    collect_calls: Arc<AtomicUsize>,
}

impl StubCollector {
    fn new(
        name: &str,
        presence: Presence,
        collected: Result<Observation, CollectionError>,
    ) -> Self {
        Self::counting(name, presence, collected, Arc::new(AtomicUsize::new(0)))
    }

    fn counting(
        name: &str,
        presence: Presence,
        collected: Result<Observation, CollectionError>,
        collect_calls: Arc<AtomicUsize>,
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
        self.collect_calls.fetch_add(1, Ordering::SeqCst);
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
    let calls = Arc::new(AtomicUsize::new(0));
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::counting(
        "nginx",
        Presence::Absent,
        Ok(Observation::null()),
        Arc::clone(&calls),
    ))];

    // Act
    fingerprint_host::run(&collectors).expect("one collector is consistent");

    // Assert
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "an absent subject must not be read"
    );
}

#[test]
fn a_present_collector_is_asked_to_collect_once() {
    // Arrange
    let calls = Arc::new(AtomicUsize::new(0));
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::counting(
        "mounts",
        Presence::Present,
        Ok(Observation::null()),
        Arc::clone(&calls),
    ))];

    // Act
    fingerprint_host::run(&collectors).expect("one collector is consistent");

    // Assert
    assert_eq!(calls.load(Ordering::SeqCst), 1);
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
    let calls = Arc::new(AtomicUsize::new(0));
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::counting(
        "postgres",
        Presence::Undetermined {
            reason: "pg_isready timed out after 5s".to_owned(),
        },
        Ok(Observation::null()),
        Arc::clone(&calls),
    ))];

    // Act
    fingerprint_host::run(&collectors).expect("one collector is consistent");

    // Assert
    assert_eq!(calls.load(Ordering::SeqCst), 0);
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

/// A caller that records what it was told, which is all a progress sink ever does.
struct Recording {
    events: Arc<Mutex<Vec<String>>>,
}

impl fingerprint_host::RunProgress for Recording {
    fn collector_started(&self, name: &FacetName) {
        self.events
            .lock()
            .expect("no test panics while holding this")
            .push(format!("started {}", name.as_str()));
    }

    fn collector_finished(&self, name: &FacetName, outcome: &FacetOutcome) {
        let status = match outcome {
            FacetOutcome::Ok { .. } => "ok",
            FacetOutcome::Absent => "absent",
            FacetOutcome::Error { .. } => "error",
        };
        self.events
            .lock()
            .expect("no test panics while holding this")
            .push(format!("finished {} {status}", name.as_str()));
    }
}

#[test]
fn an_absent_collector_is_still_reported_as_finished() {
    // Arrange: a caller timing a run needs every collector accounted for, not only the ones
    // that had something to say. Otherwise a facet that is absent looks like one that hung.
    let events = Arc::new(Mutex::new(Vec::new()));
    let collectors: Vec<Box<dyn Collector>> = vec![
        Box::new(StubCollector::new(
            "mounts",
            Presence::Present,
            Ok(Observation::null()),
        )),
        Box::new(StubCollector::new(
            "nginx",
            Presence::Absent,
            Ok(Observation::null()),
        )),
    ];

    // Act
    fingerprint_host::run_reporting(
        &collectors,
        &Recording {
            events: Arc::clone(&events),
        },
    )
    .expect("distinct facet names");

    // Assert: sorted, because collectors run concurrently and the order these arrive in is
    // whichever worker got there. Asserting the raw order here would be a test that passes by
    // luck on a two-collector run and fails on a busy machine.
    let mut reported = events
        .lock()
        .expect("no test panics while holding this")
        .clone();
    reported.sort();
    assert_eq!(
        reported,
        vec![
            "finished mounts ok",
            "finished nginx absent",
            "started mounts",
            "started nginx",
        ]
    );
}

#[test]
fn an_unreported_run_is_what_run_has_always_been() {
    // Arrange
    let collectors: Vec<Box<dyn Collector>> = vec![Box::new(StubCollector::new(
        "mounts",
        Presence::Present,
        Ok(Observation::null()),
    ))];

    // Act & Assert: the sink is optional, so nothing outside this crate has to grow a
    // parameter it does not care about.
    assert!(fingerprint_host::run(&collectors).is_ok());
}

/// A collector that records how many of its kind were running at the same moment.
///
/// The only honest way to test concurrency: a stub that observes it rather than a timing
/// comparison, which would pass on a slow machine and fail on a fast one.
struct Overlapping {
    name: FacetName,
    identity: CollectorIdentity,
    concurrency: Concurrency,
    in_flight: Arc<AtomicUsize>,
    high_water: Arc<AtomicUsize>,
}

impl Overlapping {
    fn new(
        name: &str,
        concurrency: Concurrency,
        in_flight: Arc<AtomicUsize>,
        high_water: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            name: FacetName::new(name).expect("test facet names should be legal"),
            identity: CollectorIdentity::new(
                CollectorId::new(name).expect("test collector ids should be legal"),
                CollectorVersion::new("1").expect("test collector versions should be legal"),
            ),
            concurrency,
            in_flight,
            high_water,
        }
    }
}

impl Collector for Overlapping {
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
        Presence::Present
    }

    fn concurrency(&self) -> Concurrency {
        self.concurrency
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let running = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.high_water.fetch_max(running, Ordering::SeqCst);

        // Long enough that a pool of workers overlaps and short enough not to slow the suite.
        std::thread::sleep(std::time::Duration::from_millis(60));

        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(Observation::null())
    }
}

fn overlapping(
    names: &[&str],
    concurrency: Concurrency,
    in_flight: &Arc<AtomicUsize>,
    high_water: &Arc<AtomicUsize>,
) -> Vec<Box<dyn Collector>> {
    names
        .iter()
        .map(|name| {
            Box::new(Overlapping::new(
                name,
                concurrency,
                Arc::clone(in_flight),
                Arc::clone(high_water),
            )) as Box<dyn Collector>
        })
        .collect()
}

#[test]
fn shared_collectors_run_at_the_same_time() {
    // Arrange: four collectors that each take 60 ms. Run one after another that is 240 ms of
    // waiting for subprocesses to answer, which measured as 83% of a real run.
    let in_flight = Arc::new(AtomicUsize::new(0));
    let high_water = Arc::new(AtomicUsize::new(0));
    let collectors = overlapping(
        &["alpha", "bravo", "charlie", "delta"],
        Concurrency::Shared,
        &in_flight,
        &high_water,
    );

    // Act
    fingerprint_host::run(&collectors).expect("distinct facet names");

    // Assert: observed rather than timed, so this cannot pass by accident on a slow machine.
    assert!(
        high_water.load(Ordering::SeqCst) > 1,
        "no two collectors ever overlapped"
    );
}

#[test]
fn an_exclusive_collector_never_overlaps_with_anything() {
    // Arrange: the filesystem walk is exclusive, and this is why. It observes every mount, so
    // a temp file another collector's subprocess created and deleted while it walked would be
    // recorded in one run and not the next — which is the byte-identical contract, gone.
    let in_flight = Arc::new(AtomicUsize::new(0));
    let high_water = Arc::new(AtomicUsize::new(0));
    let mut collectors = overlapping(
        &["alpha", "bravo", "charlie"],
        Concurrency::Shared,
        &in_flight,
        &high_water,
    );
    let alone = Arc::new(AtomicUsize::new(0));
    collectors.extend(overlapping(
        &["filesystem"],
        Concurrency::Exclusive,
        &in_flight,
        &alone,
    ));

    // Act
    fingerprint_host::run(&collectors).expect("distinct facet names");

    // Assert: the shared three overlapped, and while the exclusive one ran nothing else did.
    assert!(
        high_water.load(Ordering::SeqCst) > 1,
        "the shared collectors should still overlap"
    );
    assert_eq!(
        alone.load(Ordering::SeqCst),
        1,
        "something else was running during the exclusive collector"
    );
}

#[test]
fn a_concurrent_run_reports_every_collector_exactly_once() {
    // Arrange
    let events = Arc::new(Mutex::new(Vec::new()));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let high_water = Arc::new(AtomicUsize::new(0));
    let collectors = overlapping(
        &["zulu", "alpha", "mike"],
        Concurrency::Shared,
        &in_flight,
        &high_water,
    );

    // Act
    fingerprint_host::run_reporting(
        &collectors,
        &Recording {
            events: Arc::clone(&events),
        },
    )
    .expect("distinct facet names");

    // Assert: order is completion order and deliberately not reimposed, because the point of
    // hearing that a collector started is to say so while it is still running. What is
    // contractual is that every collector is accounted for exactly once, so a caller timing
    // the run cannot silently lose one. A stable report is the caller's job, by sorting.
    let mut reported = events
        .lock()
        .expect("no test panics while holding this")
        .clone();
    reported.sort();
    assert_eq!(
        reported,
        vec![
            "finished alpha ok",
            "finished mike ok",
            "finished zulu ok",
            "started alpha",
            "started mike",
            "started zulu",
        ]
    );
}

#[test]
fn a_concurrent_run_still_sorts_the_document_by_facet_name() {
    // Arrange
    let in_flight = Arc::new(AtomicUsize::new(0));
    let high_water = Arc::new(AtomicUsize::new(0));
    let collectors = overlapping(
        &["zulu", "alpha", "mike"],
        Concurrency::Shared,
        &in_flight,
        &high_water,
    );

    // Act
    let fingerprint = fingerprint_host::run(&collectors).expect("distinct facet names");

    // Assert: the document's order is a contract and must not depend on which worker won.
    let names: Vec<&str> = fingerprint
        .facets()
        .iter()
        .map(|facet| facet.name.as_str())
        .collect();
    assert_eq!(names, vec!["alpha", "mike", "zulu"]);
}
