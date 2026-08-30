//! Which box this run is describing.

mod support;

use rastro::collectors::HostCollector;
use rastro_collector::Collector;
use support::observation::{field, text};

#[test]
fn the_host_facet_reports_the_hostname_it_was_given() {
    // Arrange: read in the composition root rather than here, because the default output
    // filename carries the same hostname and the two must not disagree.
    let collector = HostCollector::reading(Ok("mr-d0-pgsql-01".to_owned()));

    // Act
    let observed = collector.collect().expect("a readable hostname");

    // Assert
    assert_eq!(text(&field(&observed, "hostname")), "mr-d0-pgsql-01");
}

#[test]
fn an_unreadable_hostname_is_an_error_facet_rather_than_an_absent_one() {
    // Arrange: a failure to *read* the host is not evidence that there isn't one, so it
    // surfaces as a recorded error on a present facet.
    let collector =
        HostCollector::reading(Err("could not read /proc/sys/kernel/hostname".to_owned()));

    // Act
    let refused = collector.collect();

    // Assert
    let message = refused.expect_err("an unreadable hostname").to_string();
    assert!(message.contains("hostname"), "got {message}");
}
