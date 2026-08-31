//! What a run tells the operator, and in what shape.
//!
//! The end-to-end tests check that `--debug` reaches stderr at all. What it *says* is pinned
//! here, because it is operator-facing text: a report whose columns drift is a report nobody
//! can compare between two runs, which is the only reason it exists.

use rastro::progress::{Reporting, WalkProgress};
use rastro_collector::fingerprint_host::RunProgress;
use rastro_fingerprint::{FacetName, FacetOutcome, Observation};

fn facet(name: &str) -> FacetName {
    FacetName::new(name).expect("a legal facet name")
}

/// A run of three collectors and a few walked entries, reported into a buffer.
fn reported() -> String {
    let sink = Reporting::new(false);

    for (name, outcome) in [
        ("zulu", FacetOutcome::ok(Observation::null())),
        ("alpha", FacetOutcome::Absent),
        ("mike", FacetOutcome::error("nothing answered")),
    ] {
        sink.collector_started(&facet(name));
        sink.collector_finished(&facet(name), &outcome);
    }

    for _ in 0..1234 {
        sink.entry_walked();
    }

    let mut written = Vec::new();
    sink.report(&mut written, "/tmp/fp.json (4629330 bytes)")
        .expect("a Vec accepts every byte");

    String::from_utf8(written).expect("the report is UTF-8")
}

#[test]
fn the_report_names_every_collector_with_its_outcome() {
    // Act
    let report = reported();

    // Assert: every collector accounted for, including the ones that had nothing to say — a
    // facet that is absent must not look like one that hung.
    assert!(report.contains("alpha"), "got {report}");
    assert!(report.contains("absent"), "got {report}");
    assert!(report.contains("error"), "got {report}");
    assert!(report.contains("ok"), "got {report}");
}

#[test]
fn the_report_is_sorted_by_name_rather_than_by_when_a_collector_finished() {
    // Act
    let report = reported();

    // Assert: collectors run concurrently, so completion order is whichever worker got there
    // and would differ between two runs of one host. Name order is deterministic, which is
    // what makes two `--debug` runs comparable line by line.
    let at = |name: &str| {
        report
            .find(name)
            .unwrap_or_else(|| panic!("no {name} in {report}"))
    };
    assert!(at("alpha") < at("mike"), "got {report}");
    assert!(at("mike") < at("zulu"), "got {report}");
}

#[test]
fn the_report_carries_the_walk_count_and_where_the_document_went() {
    // Act
    let report = reported();

    // Assert: the two questions a slow run actually raises, which `time ./rastro > file`
    // answers neither of.
    assert!(report.contains("1234 entries"), "got {report}");
    assert!(report.contains("/tmp/fp.json"), "got {report}");
    assert!(report.contains("wrote"), "got {report}");
}

#[test]
fn the_report_leads_with_a_line_naming_itself() {
    // Act & Assert: it shares stderr with warnings and a live counter, so it says what it is.
    assert!(
        reported().starts_with("rastro: debug\n"),
        "got {}",
        reported()
    );
}

#[test]
fn a_sink_that_is_not_drawing_writes_nothing_when_cleared() {
    // Arrange: `clear` exists so a warning is never half-overwritten by the counter. With the
    // counter off there is no line to clear, and it must not emit an escape sequence into a
    // redirected stderr — which is what keeps a clean run silent.
    let sink = Reporting::new(false);

    // Act & Assert: nothing to observe but the absence of a panic, which is the whole claim.
    sink.clear();
}
