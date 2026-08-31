//! Which box this run is describing.

mod support;

use rastro::collectors::HostCollector;
use rastro_collector::{Collector, CollectorCategory, Presence};
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

#[test]
fn the_host_collector_reads_the_hostname_itself_when_nobody_hands_it_one() {
    // Arrange: the constructor `built_in()` uses when there is no composition root to take
    // the reading, which is how an outside caller gets a working collector out of the port.
    let collector = HostCollector::new();

    // Assert: what it declares, not what it read. Whether `/proc/sys/kernel/hostname` exists
    // is the host's business and differs between a Linux runner and a developer's Mac, but a
    // collector's own identity is the same on every box, and it is what the envelope keys the
    // facet by.
    assert_eq!(collector.name().as_str(), "host");
    assert_eq!(collector.identity().id.as_str(), "host");
    assert_eq!(collector.category(), CollectorCategory::Metadata);
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn the_default_host_collector_is_the_one_that_reads_for_itself() {
    // Act & Assert: `Default` exists because a collector with a no-argument constructor
    // should satisfy it, and it must not become a second, quietly different collector.
    assert_eq!(
        HostCollector::default().identity().version.as_str(),
        HostCollector::new().identity().version.as_str()
    );
}
