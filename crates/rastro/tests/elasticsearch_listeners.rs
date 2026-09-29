//! The sockets a node listens on, read from a `/proc` a test built.
//!
//! The node's own `/proc/<pid>/net` is read rather than the box's, because it is the table of
//! the node's network namespace: for a node in a container, the only table its listeners are in.

use std::os::unix::fs::symlink;
use std::path::Path;

use rastro::collectors::elasticsearch::NodeListener;

mod support;

use support::fs_tree::{scratch_tree, write};

const HEADER: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
const V6_WILDCARD: &str = "00000000000000000000000000000000";
const V4_LOOPBACK: &str = "0100007F";
const V4_ANY: &str = "00000000";

fn row(address: &str, port: u16, state: &str, inode: u64) -> String {
    let remote = "0".repeat(address.len());
    format!(
        "   0: {address}:{port:04X} {remote}:0000 {state} 00000000:00000000 00:00000000 00000000  1000        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
    )
}

fn hold(proc: &Path, descriptor: &str, target: &str) {
    symlink(target, proc.join("600/fd").join(descriptor)).expect("a writable fixture");
}

#[test]
fn read_in_keeps_only_the_sockets_the_node_itself_holds() {
    // Arrange: a namespace's table lists every process's sockets, the node's and a neighbour's.
    let proc = scratch_tree("elasticsearch-listeners-held", &["600/fd", "600/net"]);
    write(
        &proc,
        "600/net/tcp6",
        &format!(
            "{HEADER}{}{}{}",
            row(V6_WILDCARD, 9300, "0A", 41),
            row(V6_WILDCARD, 9200, "0A", 42),
            row(V6_WILDCARD, 5601, "0A", 99),
        ),
    );
    write(&proc, "600/net/tcp", HEADER);
    hold(&proc, "3", "socket:[41]");
    hold(&proc, "4", "socket:[42]");
    hold(&proc, "5", "/usr/share/elasticsearch/lib/server.jar");

    // Act
    let listeners = NodeListener::read_in(&proc, 600).expect("readable listeners");

    // Assert
    let ports: Vec<u16> = listeners
        .iter()
        .map(|listener| listener.port.as_u16())
        .collect();
    assert_eq!(ports, [9200, 9300]);
    assert!(
        listeners
            .iter()
            .all(|listener| listener.host.as_str() == "::")
    );
}

#[test]
fn read_in_ignores_a_connection_on_the_listening_port() {
    // Arrange: an accepted client has the listener's port as its own local port.
    let proc = scratch_tree(
        "elasticsearch-listeners-established",
        &["600/fd", "600/net"],
    );
    write(
        &proc,
        "600/net/tcp",
        &format!(
            "{HEADER}{}{}",
            row(V4_LOOPBACK, 9200, "0A", 42),
            row(V4_LOOPBACK, 9200, "01", 43),
        ),
    );
    hold(&proc, "4", "socket:[42]");
    hold(&proc, "5", "socket:[43]");

    // Act
    let listeners = NodeListener::read_in(&proc, 600).expect("readable listeners");

    // Assert
    assert_eq!(listeners.len(), 1);
    assert_eq!(listeners[0].host.as_str(), "127.0.0.1");
}

#[test]
fn read_in_reads_both_families() {
    // Arrange
    let proc = scratch_tree("elasticsearch-listeners-families", &["600/fd", "600/net"]);
    write(
        &proc,
        "600/net/tcp",
        &format!("{HEADER}{}", row(V4_ANY, 9200, "0A", 42)),
    );
    write(
        &proc,
        "600/net/tcp6",
        &format!("{HEADER}{}", row(V6_WILDCARD, 9200, "0A", 43)),
    );
    hold(&proc, "4", "socket:[42]");
    hold(&proc, "5", "socket:[43]");

    // Act
    let listeners = NodeListener::read_in(&proc, 600).expect("readable listeners");

    // Assert
    let hosts: Vec<&str> = listeners
        .iter()
        .map(|listener| listener.host.as_str())
        .collect();
    assert_eq!(hosts, ["0.0.0.0", "::"]);
}

#[test]
fn read_in_refuses_descriptors_it_cannot_list() {
    // Arrange: a file where the directory should be fails for root too, and stands for an
    // unprivileged run looking at another user's process.
    let proc = scratch_tree("elasticsearch-listeners-refused", &["600/net"]);
    write(&proc, "600/fd", "");
    write(
        &proc,
        "600/net/tcp",
        &format!("{HEADER}{}", row(V4_ANY, 9200, "0A", 42)),
    );

    // Act
    let unread = NodeListener::read_in(&proc, 600).expect_err("unlistable descriptors");

    // Assert
    assert!(
        unread.reason().contains("descriptors"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_refuses_a_namespace_with_no_table_it_can_read() {
    // Arrange
    let proc = scratch_tree("elasticsearch-listeners-no-table", &["600/fd", "600/net"]);
    hold(&proc, "4", "socket:[42]");

    // Act
    let unread = NodeListener::read_in(&proc, 600).expect_err("no table");

    // Assert: no table read is not a node listening on nothing.
    assert!(unread.reason().contains("net/tcp"), "{}", unread.reason());
}

#[test]
fn read_in_refuses_a_table_it_cannot_parse() {
    // Arrange: a row it cannot read is a row it cannot promise it would have reported.
    let proc = scratch_tree("elasticsearch-listeners-malformed", &["600/fd", "600/net"]);
    write(
        &proc,
        "600/net/tcp",
        &format!("{HEADER}   0: not-an-address 0A\n"),
    );
    hold(&proc, "4", "socket:[42]");

    // Act
    let unread = NodeListener::read_in(&proc, 600).expect_err("a malformed table");

    // Assert
    assert!(
        unread.reason().contains("could not be parsed"),
        "{}",
        unread.reason()
    );
}
