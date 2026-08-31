//! Reading the host's networking, without needing an `ip` to run.
//!
//! The fixtures are real objects from `ip -j` on the development box, trimmed. Two of them
//! carry the cases that drove the design: a DHCP address whose lifetime counts down, and an
//! IPv6 route from a router advertisement that has an expiry and no scope. A third came from
//! a production host and cost a whole facet: a default route that iproute2 prints no
//! `protocol` for.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::network::{AddressLifetime, Ip, NetworkCollector, NetworkState, Route};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};
use support::fs_tree::scratch_tree;
use support::observation::{field, items_of, keys_of};
/// Real interfaces: the loopback, a NIC with a static address, and a NIC on DHCP.
const INTERFACES: &str = r#"[
  {"ifindex":1,"ifname":"lo","flags":["LOOPBACK","UP","LOWER_UP"],"mtu":65536,
   "operstate":"UNKNOWN","link_type":"loopback","address":"00:00:00:00:00:00",
   "addr_info":[
     {"family":"inet","local":"127.0.0.1","prefixlen":8,"scope":"host",
      "valid_life_time":4294967295,"preferred_life_time":4294967295},
     {"family":"inet6","local":"::1","prefixlen":128,"scope":"host",
      "valid_life_time":4294967295,"preferred_life_time":4294967295}]},
  {"ifindex":2,"ifname":"enp0s8","flags":["BROADCAST","MULTICAST","UP","LOWER_UP"],
   "mtu":1500,"operstate":"UP","link_type":"ether","address":"08:00:27:56:7f:78",
   "addr_info":[
     {"family":"inet","local":"10.0.2.15","prefixlen":24,"scope":"global","dynamic":true,
      "valid_life_time":62922,"preferred_life_time":62922}]},
  {"ifindex":3,"ifname":"enp0s9","flags":["BROADCAST","MULTICAST","UP","LOWER_UP"],
   "mtu":1500,"operstate":"UNKNOWN","link_type":"ether","address":"08:00:27:a0:9c:dd",
   "addr_info":[
     {"family":"inet","local":"192.168.56.103","prefixlen":24,"scope":"global",
      "valid_life_time":4294967295,"preferred_life_time":4294967295},
     {"family":"inet6","local":"fe80::a00:27ff:fea0:9cdd","prefixlen":64,"scope":"link",
      "valid_life_time":4294967295,"preferred_life_time":4294967295}]}
]"#;

const IPV4_ROUTES: &str = r#"[
  {"dst":"default","gateway":"10.0.2.2","dev":"enp0s8","protocol":"dhcp",
   "prefsrc":"10.0.2.15","metric":100,"flags":[]},
  {"dst":"10.0.2.0/24","dev":"enp0s8","protocol":"kernel","scope":"link",
   "prefsrc":"10.0.2.15","metric":100,"flags":[]},
  {"dst":"192.168.56.0/24","dev":"enp0s9","protocol":"kernel","scope":"link",
   "prefsrc":"192.168.56.103","flags":[]}
]"#;

const IPV6_ROUTES: &str = r#"[
  {"dst":"fe80::/64","dev":"enp0s8","protocol":"kernel","metric":256,"pref":"medium","flags":[]},
  {"dst":"default","gateway":"fe80::2","dev":"enp0s8","protocol":"ra","metric":100,
   "pref":"medium","expires":1708,"flags":[]}
]"#;

/// `ip -d -j -4 route show` on a Debian host whose gateway comes from ifupdown, verbatim.
///
/// The default route here is the one that cost the facet. Nothing installed it that declared
/// a protocol, so the kernel holds `RTPROT_BOOT` and `RT_SCOPE_UNIVERSE`, the two values
/// iproute2 treats as not worth printing. The `type` key arrives with `-d` and rastro ignores
/// it; it is kept in the fixture because trimming it would hide that it is there.
const IPV4_ROUTES_WITH_A_BOOT_DEFAULT: &str = r#"[
  {"type":"unicast","dst":"default","gateway":"172.21.9.1","dev":"ens18","protocol":"boot",
   "scope":"global","flags":["onlink"]},
  {"type":"unicast","dst":"172.21.9.0/25","dev":"ens18","protocol":"kernel","scope":"link",
   "prefsrc":"172.21.9.54","flags":[]}
]"#;

/// The same two routes as the same `ip` prints them without `-d`.
///
/// Not a hypothetical: this is what the collector was reading in production, and the first
/// object is where it stopped.
const IPV4_ROUTES_WITHOUT_DETAILS: &str = r#"[
  {"dst":"default","gateway":"172.21.9.1","dev":"ens18","flags":["onlink"]},
  {"dst":"172.21.9.0/25","dev":"ens18","protocol":"kernel","scope":"link",
   "prefsrc":"172.21.9.54","flags":[]}
]"#;

fn state() -> NetworkState {
    Ip::parse(INTERFACES, IPV4_ROUTES, IPV6_ROUTES).expect("these fixtures are well formed")
}

fn interface(state: &NetworkState, name: &str) -> rastro::collectors::network::NetworkInterface {
    state
        .interfaces()
        .get(&rastro::collectors::network::InterfaceName::new(name).expect("a legal name"))
        .unwrap_or_else(|| panic!("expected an interface named {name}"))
        .clone()
}

fn route_to(state: &NetworkState, destination: &str, protocol: &str) -> Route {
    state
        .routes()
        .iter()
        .find(|route| {
            route.destination.as_str() == destination && route.protocol.as_str() == protocol
        })
        .unwrap_or_else(|| panic!("expected a {protocol} route to {destination}"))
        .clone()
}

#[test]
fn parse_reads_an_interface_and_its_addresses_from_one_call() {
    // Act: `ip -j addr show` reports every field `ip link show` would, so there is no
    // second run and no join.
    let enp0s8 = interface(&state(), "enp0s8");

    // Assert
    assert_eq!(enp0s8.index, 2);
    assert_eq!(
        enp0s8
            .hardware_address
            .as_ref()
            .map(|address| address.as_str()),
        Some("08:00:27:56:7f:78")
    );
    assert_eq!(enp0s8.link_type.as_str(), "ether");
    assert_eq!(enp0s8.maximum_transmission_unit, 1500);
    assert_eq!(enp0s8.operational_state.as_str(), "UP");
    assert_eq!(enp0s8.addresses.len(), 1);
}

#[test]
fn parse_sorts_the_flags() {
    // Act: the kernel emits them in a fixed bit order today, which is the kind of
    // stability that holds until it does not.
    let lo = interface(&state(), "lo");

    // Assert
    assert_eq!(
        lo.flags
            .iter()
            .map(|flag| flag.as_str())
            .collect::<Vec<&str>>(),
        ["LOOPBACK", "LOWER_UP", "UP"]
    );
}

#[test]
fn parse_decodes_the_forever_sentinel_as_permanent() {
    // Act: `4294967295` is the kernel's sentinel, not a lifetime of 136 years.
    let lo = interface(&state(), "lo");

    // Assert
    assert_eq!(lo.addresses[0].valid_lifetime, AddressLifetime::Permanent);
    assert!(!lo.addresses[0].dynamic);
}

#[test]
fn parse_reads_a_leased_lifetime_as_a_countdown() {
    // Act: the DHCP address on the development box reads 62922 on one run and less on the
    // next.
    let enp0s8 = interface(&state(), "enp0s8");

    // Assert
    assert_eq!(
        enp0s8.addresses[0].valid_lifetime,
        AddressLifetime::Leased {
            remaining_seconds: 62922
        }
    );
    assert!(enp0s8.addresses[0].dynamic);
}

#[test]
fn a_lifetimes_permanence_survives_the_diffable_view_and_its_countdown_does_not() {
    // Arrange: this is the whole reason the lifetime is a type rather than a number. An
    // address changing from permanent to leased is a real change in how the box gets its
    // addressing, and marking the raw number volatile would have hidden it.
    let observation = Observation::from(&state());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Act
    let addresses = items_of(&field(
        &field(&field(&diffable, "interfaces"), "enp0s8"),
        "addresses",
    ));
    let lifetime = field(&addresses[0], "valid_lifetime");

    // Assert
    assert_eq!(keys_of(&lifetime), ["permanent"]);
    assert_eq!(
        field(&lifetime, "permanent").content(),
        &Content::Scalar(Scalar::Boolean(false))
    );
}

#[test]
fn a_permanent_lifetime_is_still_marked_volatile_where_it_is_absent() {
    // Act: otherwise an address that started expiring would show `null` becoming a number.
    let observation = Observation::from(&state());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");
    let addresses = items_of(&field(
        &field(&field(&diffable, "interfaces"), "lo"),
        "addresses",
    ));

    // Assert
    assert_eq!(
        keys_of(&field(&addresses[0], "valid_lifetime")),
        ["permanent"]
    );
}

#[test]
fn parse_reads_an_interface_with_no_dynamic_key_as_not_dynamic() {
    // Act: `ip` omits the key entirely rather than writing `false`.
    let lo = interface(&state(), "lo");

    // Assert
    assert!(!lo.addresses[0].dynamic);
}

#[test]
fn parse_sorts_the_addresses_on_an_interface() {
    // Act
    let enp0s9 = interface(&state(), "enp0s9");

    // Assert: `inet` sorts before `inet6`, so the order is the model's rather than the
    // kernel's.
    assert_eq!(
        enp0s9
            .addresses
            .iter()
            .map(|address| address.family.as_str())
            .collect::<Vec<&str>>(),
        ["inet", "inet6"]
    );
}

#[test]
fn parse_refuses_a_prefix_wider_than_any_address_family() {
    // Act: a number above 128 means the address and its prefix were read out of the wrong
    // fields.
    let broken = r#"[{"ifindex":1,"ifname":"lo","link_type":"loopback","mtu":65536,
      "operstate":"UP","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":200,
      "valid_life_time":1,"preferred_life_time":1}]}]"#;

    // Assert
    assert!(Ip::parse(broken, "[]", "[]").is_err());
}

#[test]
fn parse_reads_both_routing_tables_into_one_list() {
    // Act: three IPv4 routes and two IPv6 ones.
    let state = state();

    // Assert: `ip route show` with no family would have reported the three and said
    // nothing about the two.
    assert_eq!(state.routes().len(), 5);
}

#[test]
fn parse_reads_a_route_with_a_gateway() {
    // Act
    let default = route_to(&state(), "default", "dhcp");

    // Assert
    assert_eq!(
        default.gateway.as_ref().map(|gateway| gateway.as_str()),
        Some("10.0.2.2")
    );
    assert_eq!(
        default.device.as_ref().map(|device| device.as_str()),
        Some("enp0s8")
    );
    assert_eq!(default.metric, Some(100));
}

#[test]
fn parse_records_no_gateway_for_a_directly_attached_network() {
    // Act: such a route needs no next hop, and inventing one would put a value in the
    // fingerprint the kernel never reported.
    let attached = route_to(&state(), "10.0.2.0/24", "kernel");

    // Assert
    assert_eq!(attached.gateway, None);
    assert_eq!(
        attached.scope.as_ref().map(|scope| scope.as_str()),
        Some("link")
    );
}

#[test]
fn parse_reads_an_ipv6_route_with_a_preference_and_no_scope() {
    // Act: an IPv4 route has a scope and no preference, and this is the mirror image.
    let advertised = route_to(&state(), "default", "ra");

    // Assert
    assert_eq!(
        advertised.preference.as_ref().map(|pref| pref.as_str()),
        Some("medium")
    );
    assert_eq!(advertised.scope, None);
    assert_eq!(advertised.expires_seconds, Some(1708));
}

#[test]
fn a_routes_expiry_does_not_reach_the_diffable_view() {
    // Act: it counts down, so it is weather rather than configuration.
    let observation = Observation::from(&state());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    for route in items_of(&field(&diffable, "routes")) {
        assert!(
            !keys_of(&route).contains(&"expires_seconds".to_owned()),
            "an expiry must not reach the diffable view, got {:?}",
            keys_of(&route)
        );
    }
}

#[test]
fn the_protocol_that_installed_a_route_survives_the_diffable_view() {
    // Act: it is the field that says whether a route is configuration or weather, so it
    // is the one a diff most needs.
    let observation = Observation::from(&state());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    let protocols: Vec<String> = items_of(&field(&diffable, "routes"))
        .iter()
        .map(|route| match field(route, "protocol").content() {
            Content::Scalar(Scalar::Text(value)) => value.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect();
    assert!(protocols.contains(&"ra".to_owned()));
    assert!(protocols.contains(&"kernel".to_owned()));
    assert!(protocols.contains(&"dhcp".to_owned()));
}

#[test]
fn parse_sorts_the_routes() {
    // Act
    let state = state();

    // Assert
    let mut sorted = state.routes().to_vec();
    sorted.sort();
    assert_eq!(state.routes(), sorted.as_slice());
}

#[test]
fn parse_accepts_an_empty_route_table() {
    // Act: `ip -6 route show` prints nothing at all on a box with IPv6 disabled, which is
    // a host without IPv6 routes rather than a broken run.
    let state = Ip::parse(INTERFACES, IPV4_ROUTES, "").expect("no output is no routes");

    // Assert
    assert_eq!(state.routes().len(), 3);
}

#[test]
fn parse_refuses_an_interface_reported_twice() {
    // Arrange: the kernel cannot produce one, so it means rastro misread the output.
    let repeated = r#"[
      {"ifindex":1,"ifname":"lo","link_type":"loopback","mtu":1,"operstate":"UP"},
      {"ifindex":2,"ifname":"lo","link_type":"loopback","mtu":1,"operstate":"UP"}
    ]"#;

    // Act & Assert
    assert!(Ip::parse(repeated, "[]", "[]").is_err());
}

#[test]
fn parse_refuses_output_that_is_not_json() {
    // Act
    let result = Ip::parse("1: lo: <LOOPBACK,UP> mtu 65536\n", "[]", "[]");

    // Assert
    let failure = result.expect_err("the text form is not JSON");
    assert!(
        failure.to_string().contains("addr show"),
        "the message must name the subcommand, got: {failure}"
    );
}

#[test]
fn parse_reads_the_protocol_of_a_route_nobody_declared_one_for() {
    // Act: `boot` is the kernel's word for "userspace installed this and named no protocol",
    // which on Debian means every static route ifupdown brings up.
    let state = Ip::parse(INTERFACES, IPV4_ROUTES_WITH_A_BOOT_DEFAULT, "[]")
        .expect("a proto-boot route is ordinary state");

    // Assert
    let default = route_to(&state, "default", "boot");
    assert_eq!(
        default.gateway.as_ref().map(|gateway| gateway.as_str()),
        Some("172.21.9.1")
    );
    assert_eq!(
        default.scope.as_ref().map(|scope| scope.as_str()),
        Some("global")
    );
}

#[test]
fn parse_refuses_a_route_whose_protocol_ip_declined_to_print() {
    // Arrange: the shape rastro read in production, before it asked for details.

    // Act
    let failure = Ip::parse(INTERFACES, IPV4_ROUTES_WITHOUT_DETAILS, "[]")
        .expect_err("a route with no protocol is output rastro cannot read");

    // Assert: loud, and naming the subcommand, because the alternative is inventing a
    // protocol for a route and calling it observed.
    assert!(
        failure.to_string().contains("-4 route show"),
        "the message must name the subcommand, got: {failure}"
    );
    assert!(
        failure.to_string().contains("protocol"),
        "the message must name the field, got: {failure}"
    );
}

/// An `ip` that suppresses exactly what iproute2 suppresses.
///
/// `print_route` omits `protocol` when the kernel's value is `RTPROT_BOOT`, and `scope` when it
/// is `RT_SCOPE_UNIVERSE`, unless details are switched on. Emulating that policy rather than
/// replaying one host's output is the point: this asserts the question rastro asks, and it is
/// the question rather than the parsing that was wrong.
fn fake_ip(tree: &str) -> CanonicalTool {
    let body = format!(
        r#"details=no
family=-4
object=route
for argument in "$@"; do
  case "$argument" in
  -d) details=yes ;;
  -6) family=-6 ;;
  addr) object=addr ;;
  esac
done
if [ "$object" = addr ]; then printf '%s' '{interfaces}'; exit 0; fi
if [ "$family" = -6 ]; then printf '%s' '[]'; exit 0; fi
if [ "$details" = yes ]; then printf '%s' '{with_details}'; exit 0; fi
printf '%s' '{without_details}'
"#,
        interfaces = INTERFACES,
        with_details = IPV4_ROUTES_WITH_A_BOOT_DEFAULT,
        without_details = IPV4_ROUTES_WITHOUT_DETAILS,
    );

    let root = scratch_tree(&format!("network-{tree}"), &[]);
    let path = root.join("ip");
    fs::write(&path, format!("#!/bin/sh\n{body}")).expect("a writable script");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).expect("an executable script");

    CanonicalTool::located_in("ip", &[root.to_str().expect("utf-8 scratch path")])
        .expect("the script should be locatable")
}

#[test]
fn read_asks_ip_for_details_so_a_proto_boot_route_survives() {
    // Arrange
    let ip = Ip::using(fake_ip("details"));

    // Act
    let state = ip
        .read()
        .expect("an ordinary Debian routing table must not fail the facet");

    // Assert: the protocol arrived, which it only can if the invocation asked for details.
    let default = route_to(&state, "default", "boot");
    assert_eq!(
        default.scope.as_ref().map(|scope| scope.as_str()),
        Some("global")
    );
}

#[test]
fn the_facet_holds_interfaces_and_routes_side_by_side() {
    // Act: a route is meaningless without the interface it leaves by.
    let observation = Observation::from(&state());

    // Assert
    assert_eq!(keys_of(&observation), ["interfaces", "routes"]);
}

#[test]
fn presence_is_undetermined_without_ip_rather_than_absent() {
    // Act: a box with no `ip` has not stopped having interfaces.
    match NetworkCollector::reading(None).presence() {
        Presence::Undetermined { reason } => assert!(
            reason.contains("cannot be told"),
            "the reason must say rastro could not see, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn presence_is_present_when_ip_is_on_the_host() {
    // Arrange
    let ip = Ip::using(CanonicalTool::located_in("sh", &["/bin"]).expect("every unix has /bin/sh"));

    // Act & Assert
    assert_eq!(
        NetworkCollector::reading(Some(ip)).presence(),
        Presence::Present
    );
}

#[test]
fn collect_fails_rather_than_reporting_empty_networking_without_ip() {
    // Act & Assert
    assert!(NetworkCollector::reading(None).collect().is_err());
}
