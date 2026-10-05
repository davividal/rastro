//! Which of a node's listeners serves HTTP, without asking any of them.
//!
//! **Measured on 7.17.24 and 8.15.3:** a node binds its transport port first and HTTP second,
//! each the lowest free port in its range, `transport.port` defaulting to `9300-9400` and
//! `http.port` to `9200-9300`. So a default node holds 9300 and 9200, and a second node in the
//! same namespace holds 9301 and 9201. The ranges overlap at 9300, which is why "the listener
//! in the HTTP range" is not an answer, and asking each listener would send HTTP to the
//! transport port, which the node logs.

use rastro::collectors::elasticsearch::{NodeListener, NodeSettings, http_endpoint};
use rastro::collectors::inet::{InetHost, PortNumber};

fn listener(host: &str, port: u16) -> NodeListener {
    NodeListener {
        port: PortNumber::parse(&port.to_string()).expect("a port"),
        host: InetHost::new(host).expect("a host"),
    }
}

fn settings(pairs: &[(&str, &str)]) -> NodeSettings {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn http_endpoint_finds_9200_on_a_default_node_holding_9300_as_well() {
    // Arrange
    let listeners = [listener("::", 9200), listener("::", 9300)];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert: an IPv6 wildcard is dialled on IPv6 loopback, which it accepts whether or not it
    // is dual-stack, a socket option `/proc` does not publish.
    assert_eq!(endpoint.port().as_u16(), 9200);
    assert_eq!(endpoint.host().as_str(), "::1");
}

#[test]
fn http_endpoint_finds_the_second_nodes_ports_in_a_shared_namespace() {
    // Arrange: measured with two 8.15 nodes in one pod.
    let listeners = [listener("::", 9201), listener("::", 9301)];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.port().as_u16(), 9201);
}

#[test]
fn http_endpoint_takes_a_pinned_port_over_the_range() {
    // Arrange
    let listeners = [listener("::", 9250), listener("::", 9300)];

    // Act
    let endpoint =
        http_endpoint(&listeners, &settings(&[("http.port", "9250")])).expect("the pinned port");

    // Assert
    assert_eq!(endpoint.port().as_u16(), 9250);
}

#[test]
fn http_endpoint_ignores_a_listener_outside_both_ranges() {
    // Arrange: 8.x's remote-cluster server port, and anything a plugin opens.
    let listeners = [
        listener("::", 9200),
        listener("::", 9300),
        listener("::", 9443),
    ];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.port().as_u16(), 9200);
}

#[test]
fn http_endpoint_dials_the_address_a_node_bound_rather_than_loopback() {
    // Arrange: `network.host` set to one interface, where loopback has nothing listening.
    let listeners = [listener("10.0.0.5", 9200), listener("10.0.0.5", 9300)];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.host().as_str(), "10.0.0.5");
}

#[test]
fn http_endpoint_prefers_loopback_among_several_bound_addresses() {
    // Arrange: the default `_local_` binds both loopbacks.
    let listeners = [
        listener("::1", 9200),
        listener("127.0.0.1", 9200),
        listener("127.0.0.1", 9300),
    ];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.host().as_str(), "127.0.0.1");
}

#[test]
fn http_endpoint_dials_ipv4_loopback_for_a_node_bound_to_both_wildcards() {
    // Arrange
    let listeners = [
        listener("0.0.0.0", 9200),
        listener("::", 9200),
        listener("::", 9300),
    ];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.host().as_str(), "127.0.0.1");
}

#[test]
fn http_endpoint_dials_ipv6_loopback_for_a_node_bound_only_to_ipv6() {
    // Arrange
    let listeners = [listener("::1", 9200), listener("::1", 9300)];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.host().as_str(), "::1");
}

#[test]
fn http_endpoint_refuses_two_candidates_rather_than_picking_one() {
    // Arrange: a transport pinned out of its default range leaves two ports in the HTTP one.
    let listeners = [
        listener("::", 9200),
        listener("::", 9201),
        listener("::", 9300),
    ];

    // Act
    let unread = http_endpoint(&listeners, &settings(&[("transport.port", "9300")]))
        .map(|_| ())
        .expect_err("two HTTP candidates");

    // Assert
    assert!(unread.reason().contains("9200"), "{}", unread.reason());
    assert!(unread.reason().contains("9201"), "{}", unread.reason());
}

#[test]
fn http_endpoint_refuses_a_node_with_no_http_listener() {
    // Arrange: a node still starting, which has bound transport and not yet HTTP.
    let listeners = [listener("::", 9300)];

    // Act
    let unread = http_endpoint(&listeners, &settings(&[]))
        .map(|_| ())
        .expect_err("no HTTP listener");

    // Assert
    assert!(unread.reason().contains("HTTP"), "{}", unread.reason());
}

#[test]
fn http_endpoint_refuses_a_port_setting_it_cannot_read() {
    // Arrange
    let listeners = [listener("::", 9200), listener("::", 9300)];

    // Act
    let unread = http_endpoint(&listeners, &settings(&[("http.port", "ninety")]))
        .map(|_| ())
        .expect_err("an unreadable port");

    // Assert
    assert!(unread.reason().contains("http.port"), "{}", unread.reason());
}

#[test]
fn http_endpoint_falls_back_to_ipv4_loopback_for_an_ipv6_wildcard() {
    // Arrange: found by review, measured on 8.19.22. A `::` listener is dual-stack by default,
    // and `::1` can be unavailable while it is bound, with IPv6 off on `lo` alone.
    let listeners = [listener("::", 9200), listener("::", 9300)];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.fallback().map(InetHost::as_str), Some("127.0.0.1"));
}

#[test]
fn http_endpoint_has_no_fallback_for_an_address_the_node_bound_itself() {
    // Arrange
    let listeners = [listener("::1", 9200), listener("::1", 9300)];

    // Act
    let endpoint = http_endpoint(&listeners, &settings(&[])).expect("one HTTP port");

    // Assert
    assert_eq!(endpoint.fallback(), None);
}

#[test]
fn http_endpoint_names_a_port_setting_it_cannot_read_and_never_its_value() {
    // Arrange: found by review. A placeholder resolves before this read, so `http.port:
    // ${TOKEN}` arrives as whatever `TOKEN` holds, and a refusal is written into the document.
    let listeners = [listener("127.0.0.1", 9200)];
    let settings = settings(&[("http.port", "s3cret-token")]);

    // Act
    let unread = http_endpoint(&listeners, &settings).expect_err("not a port");

    // Assert
    assert!(unread.reason().contains("http.port"), "{}", unread.reason());
    assert!(
        !unread.reason().contains("s3cret-token"),
        "{}",
        unread.reason()
    );
}
