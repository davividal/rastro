//! Reading the socket tables out of `/proc`, which costs the host nothing.
//!
//! Every fixture row is real output from the development box. `ss` is the obvious source
//! for this facet and it is the wrong one: `ss -t -u` makes the kernel load `udp_diag` and
//! `ss -x` loads `unix_diag`, so the tool would be changing the host it was sent to
//! describe. These tables are plain reads and load nothing, which was measured.

use rastro::collectors::sockets::{InetTable, SocketAddress, proc_net_inet, proc_net_unix};

/// Real `/proc/net/tcp`, header included: a loopback listener, a wildcard listener, a
/// listener on one of `systemd-resolved`'s stub addresses, and a connected socket that is
/// not listening at all.
const TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000   105        0 66633 1 000000006d675ddf 100 0 0 10 0
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 45643 1 00000000ee7a3cea 100 0 0 10 0
   2: 3600007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000   996        0 43875 1 000000002b6c7eb5 100 0 0 10 5
   3: 0100007F:1538 0100007F:C1A2 01 00000000:00000000 00:00000000 00000000   105        0 71234 1 000000004d675d11 100 0 0 10 0
";

/// Real `/proc/net/tcp6`: a loopback listener, and a wildcard that is one socket serving
/// both families.
const TCP6: &str = "\
   0: 00000000000000000000000001000000:0019 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 22348 1 00000000f53bcb72 100 0 0 10 20
   1: 00000000000000000000000000000000:238F 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 96835 1 00000000b8b804c7 100 0 0 10 0
";

/// Real `/proc/net/udp`. A datagram socket never listens, so its state is the one the
/// kernel uses for unconnected.
const UDP: &str = "\
15006: 3600007F:0035 00000000:0000 07 00000000:00000000 00:00000000 00000000   996        0 43874 2 000000001b2ef305 0
";

/// Real `/proc/net/udp6`, including the link-local address `systemd-networkd` binds for
/// DHCPv6.
const UDP6: &str = "\
15499: 000080FE00000000FF27000ADD9CA0FE:0222 00000000000000000000000000000000:0000 07 00000000:00000000 00:00000000 00000000   998        0 146930 2 00000000706425d8 0
";

/// Real `/proc/net/unix`, header included. A stream listener, a sequenced-packet listener,
/// a bound datagram socket, an abstract socket with no name in any filesystem, and two
/// connected sockets that are not listening.
const UNIX: &str = "\
Num       RefCount Protocol Flags    Type St Inode Path
000000004bfa98b1: 00000002 00000000 00010000 0001 01 11955 /run/systemd/journal/stdout
00000000256667e3: 00000002 00000000 00010000 0005 01 11958 /run/udev/control
000000001519133d: 00000002 00000000 00000000 0002 01 158944 /run/user/1000/systemd/notify
0000000033f7d9c3: 00000002 00000000 00000000 0002 01 22346 @/var/spool/exim4/exim_daemon_notify
0000000045c0e880: 00000003 00000000 00000000 0001 03 72721
00000000b23a9f93: 00000003 00000000 00000000 0001 03 72049 /run/systemd/journal/stdout
";

fn inet(table: InetTable, text: &str) -> Vec<rastro::collectors::sockets::SocketRow> {
    proc_net_inet::parse(table, text).expect("these fixtures are real rows")
}

fn bound(rows: &[rastro::collectors::sockets::SocketRow], port: u16) -> String {
    rows.iter()
        .find_map(|row| match &row.address {
            SocketAddress::Inet { host, port: bound } if bound.as_u16() == port => {
                Some(host.as_str().to_owned())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a socket on port {port}"))
}

#[test]
fn an_ipv4_address_is_decoded_from_its_little_endian_hexadecimal() {
    // Arrange: `0100007F` is 127.0.0.1 written as a host-order word, which is the single
    // detail that makes this table unreadable to the naked eye.

    // Act
    let rows = inet(InetTable::Tcp, TCP);

    // Assert
    assert_eq!(bound(&rows, 5432), "127.0.0.1");
    assert_eq!(bound(&rows, 53), "127.0.0.54");
}

#[test]
fn an_ipv4_wildcard_is_the_address_that_says_reachable_from_anywhere() {
    // Act
    let rows = inet(InetTable::Tcp, TCP);

    // Assert: the difference between this and 127.0.0.1 is the whole point of the facet.
    assert_eq!(bound(&rows, 22), "0.0.0.0");
}

#[test]
fn an_ipv6_address_is_decoded_word_by_word_not_byte_by_byte() {
    // Arrange: each of the four 32-bit words is host-ordered on its own, so reversing the
    // whole 16 bytes gives a plausible and entirely wrong address.

    // Act
    let rows = inet(InetTable::Udp6, UDP6);

    // Assert
    assert_eq!(bound(&rows, 546), "fe80::a00:27ff:fea0:9cdd");
}

#[test]
fn an_ipv6_address_is_written_in_its_canonical_compressed_form() {
    // Act
    let rows = inet(InetTable::Tcp6, TCP6);

    // Assert
    assert_eq!(bound(&rows, 25), "::1");
    assert_eq!(bound(&rows, 9103), "::");
}

#[test]
fn only_listening_sockets_are_taken_from_the_tcp_table() {
    // Arrange: the table holds every TCP socket, and an established connection is traffic
    // rather than state. Its presence would also make the facet differ between two runs.

    // Act
    let rows = inet(InetTable::Tcp, TCP);

    // Assert: four rows in, three listeners out.
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.state.as_str() == "LISTEN"));
}

#[test]
fn a_datagram_socket_is_recorded_as_unconnected_rather_than_listening() {
    // Arrange: UDP never listens in the TCP sense, and a bound UDP port is just as much
    // exposure as a TCP one.

    // Act
    let rows = inet(InetTable::Udp, UDP);

    // Assert
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state.as_str(), "UNCONN");
    assert_eq!(rows[0].kind.as_str(), "udp");
}

#[test]
fn a_row_carries_the_inode_that_names_its_holding_process() {
    // Arrange: the table names no process, and the inode is the only join back to one.

    // Act
    let rows = inet(InetTable::Tcp, TCP);

    // Assert
    assert_eq!(
        rows.iter().map(|row| row.inode).collect::<Vec<u64>>(),
        [66633, 45643, 43875]
    );
}

#[test]
fn a_unix_stream_listener_and_a_sequenced_packet_listener_keep_their_own_kinds() {
    // Act
    let rows = proc_net_unix::parse(UNIX).expect("these fixtures are real rows");

    // Assert
    assert_eq!(rows[0].kind.as_str(), "u_str");
    assert_eq!(rows[0].state.as_str(), "LISTEN");
    assert_eq!(rows[1].kind.as_str(), "u_seq");
    assert_eq!(rows[1].state.as_str(), "LISTEN");
}

#[test]
fn a_bound_datagram_socket_is_a_listener_without_the_accept_flag() {
    // Arrange: `SO_ACCEPTCON` is what separates LISTEN from UNCONN here, and a datagram
    // socket never sets it while still holding a name.

    // Act
    let rows = proc_net_unix::parse(UNIX).expect("these fixtures are real rows");

    // Assert
    assert_eq!(rows[2].kind.as_str(), "u_dgr");
    assert_eq!(rows[2].state.as_str(), "UNCONN");
}

#[test]
fn an_abstract_socket_keeps_the_marker_that_says_it_is_in_no_filesystem() {
    // Arrange: an abstract socket has no name on disk, so the filesystem walk cannot see
    // it and this is the only facet that records it at all.

    // Act
    let rows = proc_net_unix::parse(UNIX).expect("these fixtures are real rows");
    let paths: Vec<&str> = rows
        .iter()
        .filter_map(|row| match &row.address {
            SocketAddress::Local { path } => Some(path.as_str()),
            SocketAddress::Inet { .. } => None,
        })
        .collect();

    // Assert
    assert!(paths.contains(&"@/var/spool/exim4/exim_daemon_notify"));
}

#[test]
fn a_connected_unix_socket_is_not_a_listener() {
    // Arrange: `St` 03 is a connected socket. Six rows in the fixture, two of them
    // connected, and one of those two shares a path with a real listener, so filtering by
    // path rather than by state would report it twice.

    // Act
    let rows = proc_net_unix::parse(UNIX).expect("these fixtures are real rows");

    // Assert
    assert_eq!(rows.len(), 4);
}

#[test]
fn a_row_with_too_few_columns_is_refused_rather_than_skipped() {
    // Arrange: a socket rastro silently dropped is the worst kind of incompleteness, and a
    // short row means the table is not the one this parser was written for.

    // Act
    let result = proc_net_inet::parse(InetTable::Tcp, "   0: 00000000:0016 00000000:0000 0A\n");

    // Assert
    let failure = result.expect_err("a truncated row must not be accepted");
    assert!(
        failure.to_string().contains("columns"),
        "the message must say what was wrong, got: {failure}"
    );
}

#[test]
fn a_malformed_row_fails_even_when_its_state_would_have_skipped_it() {
    // Arrange: an established connection is skipped, but only after it parses. Checking
    // the state first would let a misread table look like a quiet one.
    let established = "   3: NOTHEX:1538 0100007F:C1A2 01 0 0 0 0 0 0 71234 1 x 100 0 0 10 0\n";

    // Act & Assert
    assert!(proc_net_inet::parse(InetTable::Tcp, established).is_err());
}

#[test]
fn an_ipv6_row_read_as_ipv4_is_refused_rather_than_truncated() {
    // Arrange: the address width is the check. Reading the first eight digits of a 16-byte
    // address would yield a real-looking and entirely wrong IPv4 address.

    // Act & Assert
    assert!(proc_net_inet::parse(InetTable::Tcp, TCP6).is_err());
}

#[test]
fn a_unix_socket_type_the_family_does_not_have_is_refused() {
    // Arrange: unix sockets have had stream, datagram and sequenced-packet for the life of
    // the interface, so a fourth means the columns were misread rather than that the
    // kernel grew one.
    let row = "0000000045c0e880: 00000002 00000000 00010000 0009 01 72721 /run/x\n";

    // Act & Assert
    assert!(proc_net_unix::parse(row).is_err());
}

#[test]
fn an_unbound_unix_socket_is_skipped_rather_than_failing_the_facet() {
    // Arrange: a socket that has been created and neither bound nor connected is listed in
    // this state with no name at all — `unix_seq_show` prints `SS_UNCONNECTED` for any
    // socket a process still holds that is not established. It is not a listener and has no
    // address to report, so it is skipped. Refusing it would fail the whole facet whenever
    // any process on the box happened to be between `socket()` and `bind()`.
    let unbound = "0000000045c0e880: 00000002 00000000 00000000 0001 01 72721\n";

    // Act
    let rows = proc_net_unix::parse(unbound).expect("an unnamed socket is not a failure");

    // Assert
    assert!(rows.is_empty());
}

#[test]
fn a_named_listener_beside_an_unbound_socket_is_still_reported() {
    // Arrange: skipping the nameless one must not cost the row next to it.
    let mixed = "0000000045c0e880: 00000002 00000000 00000000 0001 01 72721\n\
                 000000004bfa98b1: 00000002 00000000 00010000 0001 01 11955 /run/systemd/journal/stdout\n";

    // Act
    let rows = proc_net_unix::parse(mixed).expect("these are real rows");

    // Assert
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].inode, 11955);
}

#[test]
fn a_unix_path_containing_a_space_survives_the_column_split() {
    // Arrange: a unix socket path may legally contain a space, and splitting the whole line
    // on whitespace would truncate the path at the first one.
    let row = "0000000045c0e880: 00000002 00000000 00010000 0001 01 72721 /run/two words.sock\n";

    // Act
    let rows = proc_net_unix::parse(row).expect("a space is legal in a socket path");

    // Assert
    match &rows[0].address {
        SocketAddress::Local { path } => assert_eq!(path.as_str(), "/run/two words.sock"),
        other => panic!("expected a local address, got {other:?}"),
    }
}

#[test]
fn a_row_whose_inode_is_not_a_number_is_refused() {
    // Arrange: the inode is the only route from a socket to its holder, so a column that
    // does not hold one means the columns were counted wrong.
    let row = "   0: 00000000:0016 00000000:0000 0A 0 0 0 0 0 notanumber 1 x 100 0 0 10 0\n";

    // Act & Assert
    assert!(proc_net_inet::parse(InetTable::Tcp, row).is_err());
}

#[test]
fn an_address_column_with_no_port_is_refused() {
    // Arrange: `HEX:HEX` is the whole grammar of that column.
    let row = "   0: 00000000 00000000:0000 0A 0 0 0 0 0 45643 1 x 100 0 0 10 0\n";

    // Act
    let failure = proc_net_inet::parse(InetTable::Tcp, row)
        .expect_err("an address with no port must not be accepted");

    // Assert
    assert!(
        failure.to_string().contains("address and port"),
        "the message must say what was wrong, got: {failure}"
    );
}

#[test]
fn an_ipv6_address_of_the_wrong_width_is_refused() {
    // Arrange: the width is the check. Sixteen bytes is not negotiable, and a short field
    // read as an address would produce a real-looking and wrong one.
    let row = "   0: 0000000000000001:0016 00000000000000000000000000000000:0000 0A 0 0 0 0 0 45643 1 x\n";

    // Act
    let failure = proc_net_inet::parse(InetTable::Tcp6, row)
        .expect_err("a short IPv6 address must not be accepted");

    // Assert
    assert!(
        failure.to_string().contains("16-byte"),
        "the message must name the width, got: {failure}"
    );
}

#[test]
fn an_ipv6_address_that_is_not_hexadecimal_is_refused() {
    // Arrange: right width, wrong alphabet, so the length check passes and the parse must
    // catch it.
    let row = "   0: 0000000000000000000000000000ZZZZ:0016 00000000000000000000000000000000:0000 0A 0 0 0 0 0 45643 1 x\n";

    // Act & Assert
    assert!(proc_net_inet::parse(InetTable::Tcp6, row).is_err());
}

#[test]
fn an_ipv4_address_that_is_not_hexadecimal_is_refused() {
    // Arrange
    let row = "   0: ZZZZZZZZ:0016 00000000:0000 0A 0 0 0 0 0 45643 1 x 100 0 0 10 0\n";

    // Act
    let failure = proc_net_inet::parse(InetTable::Tcp, row)
        .expect_err("a non-hexadecimal address must not be accepted");

    // Assert
    assert!(
        failure.to_string().contains("address word"),
        "the message must say what was wrong, got: {failure}"
    );
}

#[test]
fn a_unix_row_with_too_few_columns_is_refused() {
    // Arrange
    let row = "0000000045c0e880: 00000002 00000000 00010000\n";

    // Act
    let failure = proc_net_unix::parse(row).expect_err("a truncated row must not be accepted");

    // Assert
    assert!(
        failure.to_string().contains("too few columns"),
        "the message must say what was wrong, got: {failure}"
    );
}

#[test]
fn a_unix_row_whose_inode_is_not_a_number_is_refused() {
    // Arrange
    let row = "0000000045c0e880: 00000002 00000000 00010000 0001 01 notanumber /run/x\n";

    // Act & Assert
    assert!(proc_net_unix::parse(row).is_err());
}

#[test]
fn a_unix_row_whose_flags_are_not_hexadecimal_are_refused() {
    // Arrange: the flags word is what separates a listener from a bound datagram socket, so
    // guessing at an unreadable one would mislabel the socket's state.
    let row = "0000000045c0e880: 00000002 00000000 NOTHEX00 0001 01 72721 /run/x\n";

    // Act
    let failure = proc_net_unix::parse(row).expect_err("unreadable flags must not be accepted");

    // Assert
    assert!(
        failure.to_string().contains("flags word"),
        "the message must say what was wrong, got: {failure}"
    );
}
