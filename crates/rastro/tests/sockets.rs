//! Reading what the host is listening on, without needing an `ss` to run.
//!
//! Every fixture row is a real row from `ss` on the development box. The address forms
//! are the point: five spellings appear there, and each one breaks a different naive
//! parser.

use rastro::collectors::sockets::{
    InetHost, ListeningSocket, SocketAddress, SocketTable, SocketsCollector, Ss, ss_address,
    ss_users,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};

/// Real internet rows: an IPv4 wildcard, an any-family wildcard, a loopback binding, an
/// address scoped to an interface, an IPv6 wildcard, and a link-local IPv6 with a scope.
const INET: &str = "\
tcp LISTEN 0      128                              0.0.0.0:22   0.0.0.0:* users:((\"sshd\",pid=24169,fd=3))
tcp LISTEN 0      4096                                   *:9100       *:* users:((\"node_exporter\",pid=44549,fd=3))
tcp LISTEN 0      200                            127.0.0.1:5432 0.0.0.0:* users:((\"postgres\",pid=44012,fd=7))
tcp LISTEN 0      4096                       127.0.0.53%lo:53   0.0.0.0:* users:((\"systemd-resolve\",pid=22920,fd=19))
tcp LISTEN 0      128                                 [::]:22      [::]:* users:((\"sshd\",pid=24169,fd=4))
udp UNCONN 0      0      [fe80::a00:27ff:fea0:9cdd]%enp0s9:546     [::]:* users:((\"systemd-network\",pid=3984,fd=23))
";

/// Real unix rows, including one held by two processes and one abstract socket.
const LOCAL: &str = "\
u_str LISTEN 0      4096                       /run/systemd/journal/stdout 11955  * 0 users:((\"systemd-journal\",pid=3989,fd=5),(\"systemd\",pid=1,fd=117))
u_dgr UNCONN 0      0                 @/var/spool/exim4/exim_daemon_notify 22346  * 0 users:((\"exim4\",pid=2636,fd=3))
u_str LISTEN 0      4096        /run/systemd/userdb/io.systemd.DynamicUser 12996  * 0 users:((\"systemd\",pid=1,fd=44))
";

fn table() -> SocketTable {
    Ss::parse(INET, LOCAL).expect("these fixtures are well formed")
}

fn bound_to(table: &SocketTable, host: &str, port: u16) -> ListeningSocket {
    table
        .sockets()
        .iter()
        .find(|socket| match &socket.address {
            SocketAddress::Inet {
                host: found,
                port: bound,
                ..
            } => found.as_str() == host && bound.as_u16() == port,
            SocketAddress::Local { .. } => false,
        })
        .unwrap_or_else(|| panic!("expected a socket bound to {host}:{port}"))
        .clone()
}

fn at_path(table: &SocketTable, path: &str) -> ListeningSocket {
    table
        .sockets()
        .iter()
        .find(|socket| match &socket.address {
            SocketAddress::Local { path: found } => found.as_str() == path,
            SocketAddress::Inet { .. } => false,
        })
        .unwrap_or_else(|| panic!("expected a socket at {path}"))
        .clone()
}

fn names_of(socket: &ListeningSocket) -> Vec<&str> {
    socket
        .processes
        .iter()
        .map(|process| process.name.as_str())
        .collect()
}

fn object_of(observation: &Observation) -> Vec<(String, Observation)> {
    match observation.content() {
        Content::Object(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn field(observation: &Observation, name: &str) -> Observation {
    object_of(observation)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("expected a {name:?} field"))
}

fn items_of(observation: &Observation) -> Vec<Observation> {
    match observation.content() {
        Content::List(items) => items.clone(),
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn parse_reads_an_ipv4_binding() {
    // Act
    let ssh = bound_to(&table(), "0.0.0.0", 22);

    // Assert
    assert_eq!(ssh.kind.as_str(), "tcp");
    assert_eq!(ssh.state.as_str(), "LISTEN");
    assert_eq!(names_of(&ssh), ["sshd"]);
}

#[test]
fn parse_splits_an_ipv6_address_on_the_last_colon_not_the_first() {
    // Act: an IPv6 address contains up to seven colons, so any other split mis-slots
    // every IPv6 row.
    let address = ss_address::parse("[fe80::a00:27ff:fea0:9cdd]%enp0s9:546")
        .expect("this is a real address form");

    // Assert
    match address {
        SocketAddress::Inet { host, port, scope } => {
            assert_eq!(host.as_str(), "fe80::a00:27ff:fea0:9cdd");
            assert_eq!(port.as_u16(), 546);
            assert_eq!(
                scope.map(|scope| scope.as_str().to_owned()),
                Some("enp0s9".to_owned())
            );
        }
        other => panic!("expected an internet address, got {other:?}"),
    }
}

#[test]
fn parse_takes_the_brackets_off_an_ipv6_address() {
    // Act: they are `ss`'s punctuation for separating the address from the port.
    let address = ss_address::parse("[::]:22").expect("a legal address");

    // Assert
    match address {
        SocketAddress::Inet { host, port, scope } => {
            assert_eq!(host.as_str(), "::");
            assert_eq!(port.as_u16(), 22);
            assert_eq!(scope, None);
        }
        other => panic!("expected an internet address, got {other:?}"),
    }
}

#[test]
fn parse_keeps_an_interface_scope_as_its_own_fact() {
    // Act: the same address scoped to a different interface is a different binding.
    let resolved = bound_to(&table(), "127.0.0.53", 53);

    // Assert
    match resolved.address {
        SocketAddress::Inet { scope, .. } => {
            assert_eq!(
                scope.map(|scope| scope.as_str().to_owned()),
                Some("lo".to_owned())
            );
        }
        other => panic!("expected an internet address, got {other:?}"),
    }
}

#[test]
fn parse_keeps_the_three_wildcard_spellings_apart() {
    // Act: `*`, `0.0.0.0` and `::` are a real difference in what a daemon asked the kernel
    // for, and normalising them would erase it.
    let table = table();

    // Assert
    assert!(bound_to(&table, "*", 9100).address != bound_to(&table, "0.0.0.0", 22).address);
    for host in ["*", "0.0.0.0", "::"] {
        assert!(
            InetHost::new(host).expect("legal").is_a_wildcard(),
            "{host} reaches the network"
        );
    }
    assert!(!InetHost::new("127.0.0.1").expect("legal").is_a_wildcard());
}

#[test]
fn parse_refuses_a_port_outside_the_range_a_port_can_hold() {
    // Act: the width is the check. A number too large means the split found the wrong
    // colon.
    let result = ss_address::parse("10.0.0.1:99999");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_reads_a_unix_socket_by_its_path() {
    // Act
    let socket = at_path(&table(), "/run/systemd/userdb/io.systemd.DynamicUser");

    // Assert
    assert_eq!(socket.kind.as_str(), "u_str");
    assert_eq!(names_of(&socket), ["systemd"]);
}

#[test]
fn parse_keeps_an_abstract_unix_socket() {
    // Act: an abstract socket has a name in no filesystem, so a filesystem walk cannot see
    // it and this facet is the only place it appears.
    let socket = at_path(&table(), "@/var/spool/exim4/exim_daemon_notify");

    // Assert
    match &socket.address {
        SocketAddress::Local { path } => assert!(path.is_abstract()),
        other => panic!("expected a local address, got {other:?}"),
    }
}

#[test]
fn parse_reads_a_socket_held_by_more_than_one_process() {
    // Act: `/run/systemd/journal/stdout` really is held by both.
    let journal = at_path(&table(), "/run/systemd/journal/stdout");

    // Assert: sorted, so the order `ss` listed them in never reaches the document.
    assert_eq!(names_of(&journal), ["systemd", "systemd-journal"]);
}

#[test]
fn parse_reads_a_socket_with_no_holding_process() {
    // Arrange: `ss` omits the column when the kernel holds a socket with no userspace
    // process behind it, or when rastro is not privileged enough to be told.
    let row = "tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*\n";

    // Act
    let table = Ss::parse(row, "").expect("a missing process column is not a failure");

    // Assert
    assert!(table.sockets()[0].processes.is_empty());
}

#[test]
fn parse_refuses_a_truncated_row() {
    // Act
    let result = Ss::parse("tcp LISTEN 0\n", "");

    // Assert
    let failure = result.expect_err("a truncated row must not be accepted");
    assert!(
        failure.to_string().contains("fields"),
        "the message must say what was wrong, got: {failure}"
    );
}

#[test]
fn parse_refuses_a_unix_row_that_reached_the_internet_grammar() {
    // Act: the two shapes are seven fields and nine, which is exactly why they are asked
    // for in two runs rather than counted apart in one.
    let unix_row = "u_str LISTEN 0 4096 /run/x 11955 * 0\n";
    let result = Ss::parse(unix_row, "");

    // Assert: read as an internet row, `11955` is not an address and port.
    assert!(result.is_err());
}

#[test]
fn parse_sorts_the_sockets() {
    // Act: `ss` walks the kernel's hash tables, whose order depends on which sockets were
    // opened when.
    let table = table();

    // Assert
    let mut sorted = table.sockets().to_vec();
    sorted.sort();
    assert_eq!(table.sockets(), sorted.as_slice());
}

#[test]
fn parse_reads_both_families_into_one_table() {
    // Act
    let table = table();

    // Assert: six internet rows and three unix rows.
    assert_eq!(table.len(), 9);
}

#[test]
fn users_parses_the_nested_triple_grammar() {
    // Act
    let processes = ss_users::parse(Some(
        "users:((\"systemd-journal\",pid=3989,fd=5),(\"systemd\",pid=1,fd=117))",
    ))
    .expect("this is the column `ss -p` writes");

    // Assert
    assert_eq!(processes.len(), 2);
    let first = processes.iter().next().expect("a process");
    assert_eq!(first.name.as_str(), "systemd");
    assert_eq!(first.process_id, 1);
    assert_eq!(first.file_descriptor, 117);
}

#[test]
fn users_refuses_a_column_that_is_not_the_process_column() {
    // Act & Assert
    assert!(ss_users::parse(Some("something-else")).is_err());
}

#[test]
fn a_process_id_is_volatile_and_its_name_is_not() {
    // Act: a pid changes every time a service restarts. `postgres` no longer holding 5432
    // is a change; `postgres` holding it under a new pid is not.
    let observation = Observation::from(&table());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert
    let holders = items_of(&field(&items_of(&diffable)[0], "processes"));
    let keys: Vec<String> = object_of(&holders[0])
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, ["name"]);
}

#[test]
fn an_address_renders_the_same_four_keys_for_either_family() {
    // Act
    let observation = Observation::from(&table());

    // Assert: a consumer never meets a key that is sometimes absent, and which family it
    // is stays readable from which keys are null.
    for socket in items_of(&observation) {
        let address = field(&socket, "address");
        let keys: Vec<String> = object_of(&address)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        assert_eq!(keys, ["host", "path", "port", "scope"]);
    }
}

#[test]
fn a_unix_socket_renders_a_null_port() {
    // Act
    let observation = Observation::from(&table());
    let local = items_of(&observation)
        .into_iter()
        .find(|socket| {
            matches!(
                field(&field(socket, "address"), "path").content(),
                Content::Scalar(Scalar::Text(_))
            )
        })
        .expect("a unix socket");

    // Assert
    assert_eq!(
        field(&field(&local, "address"), "port").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn presence_is_undetermined_without_ss_rather_than_absent() {
    // Act: a box with no `ss` has not stopped listening on anything, so `absent` would be
    // a confident lie about the box's exposure.
    let presence = SocketsCollector::reading(None).presence();

    // Assert
    match presence {
        Presence::Undetermined { reason } => assert!(
            reason.contains("cannot be told"),
            "the reason must say rastro could not see, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn presence_is_present_when_ss_is_on_the_host() {
    // Arrange
    let ss = Ss::using(
        rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
            .expect("every unix has /bin/sh"),
    );

    // Act & Assert
    assert_eq!(
        SocketsCollector::reading(Some(ss)).presence(),
        Presence::Present
    );
}

#[test]
fn collect_fails_rather_than_reporting_an_empty_table_without_ss() {
    // Act & Assert
    assert!(SocketsCollector::reading(None).collect().is_err());
}
