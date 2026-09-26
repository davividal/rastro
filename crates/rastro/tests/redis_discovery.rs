//! Which servers are on the box, what each is keyed by, and where rastro may reach it.
//!
//! Nothing here connects to anything. Discovery is `/proc` alone, so which socket a server is
//! reached on is decided from what the kernel says it holds, never from a guessed default.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::redis::{DialTarget, DiscoveredServer, ServerKind, discover};

mod support;

use support::fs_tree::{scratch_tree, write};

const TCP_HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
const UNIX_HEADER: &str = "Num       RefCount Protocol Flags    Type St Inode Path\n";

/// A listening IPv4 row: the address and port as the kernel spells them, and an inode.
fn tcp_row(address: &str, inode: u64) -> String {
    format!(
        "   0: {address} 00000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
    )
}

/// A listening IPv6 row.
fn tcp6_row(address: &str, inode: u64) -> String {
    format!(
        "   0: {address} 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
    )
}

/// A listening unix stream socket: `SO_ACCEPTCON` set, type `0001`, unconnected.
fn unix_row(path: &str, inode: u64) -> String {
    format!("0000000000000000: 00000002 00000000 00010000 0001 01 {inode} {path}\n")
}

/// 127.0.0.1:6379, where 6379 is `18EB` and the address is a host-order word.
const LOOPBACK_6379: &str = "0100007F:18EB";
const WILDCARD_6379: &str = "00000000:18EB";
/// 10.0.0.5:6379, an address of the box's own rather than loopback.
const OWN_6379: &str = "0500000A:18EB";
const LOOPBACK_6380: &str = "0100007F:18EC";
const IPV6_LOOPBACK_6379: &str = "00000000000000000000000001000000:18EB";
const IPV6_WILDCARD_6379: &str = "00000000000000000000000000000000:18EB";

/// One process on the fixture box.
struct Process<'a> {
    pid: &'a str,
    comm: &'a str,
    title: &'a str,
    /// The socket inodes it holds, or `None` for descriptors this run may not read.
    holds: Option<&'a [u64]>,
}

fn server(pid: &'static str, holds: &'static [u64]) -> Process<'static> {
    Process {
        pid,
        comm: "redis-server",
        title: "/usr/bin/redis-server 127.0.0.1:6379\0",
        holds: Some(holds),
    }
}

/// A `/proc` with the processes and socket tables named.
fn proc_with(name: &str, processes: &[Process], tcp: &str, tcp6: &str, unix: &str) -> PathBuf {
    let proc = scratch_tree(name, &["proc/net"]).join("proc");
    write(&proc, "net/tcp", &format!("{TCP_HEADER}{tcp}"));
    write(&proc, "net/tcp6", &format!("{TCP_HEADER}{tcp6}"));
    write(&proc, "net/unix", &format!("{UNIX_HEADER}{unix}"));

    for process in processes {
        let directory = proc.join(process.pid);
        fs::create_dir_all(&directory).expect("a writable scratch directory");
        write(&directory, "comm", &format!("{}\n", process.comm));
        write(&directory, "cmdline", process.title);

        match process.holds {
            Some(inodes) => {
                fs::create_dir_all(directory.join("fd")).expect("a writable scratch directory");
                for (descriptor, inode) in inodes.iter().enumerate() {
                    symlink(
                        format!("socket:[{inode}]"),
                        directory.join("fd").join((descriptor + 3).to_string()),
                    )
                    .expect("a writable scratch symlink");
                }
            }
            // Arrange: a file where the directory should be refuses the listing with
            // `ENOTDIR`, which stands for `EACCES` without depending on who runs the suite:
            // root reads through a mode, so a mode-000 directory would pass as root and fail
            // unprivileged.
            None => write(&directory, "fd", ""),
        }
    }

    proc
}

fn only(proc: &Path) -> DiscoveredServer {
    let mut discovered = discover(proc);
    assert_eq!(discovered.len(), 1, "{discovered:?}");

    discovered.remove(0)
}

fn listeners_of(server: &DiscoveredServer) -> Vec<String> {
    server
        .listeners
        .iter()
        .map(|listener| listener.to_string())
        .collect()
}

#[test]
fn a_server_is_keyed_by_its_port_and_lists_every_socket_it_holds() {
    // Arrange: Debian's `bind 127.0.0.1 -::1`, which is two sockets on one port.
    let proc = proc_with(
        "redis-discovery-loopback",
        &[server("412", &[1001, 1002])],
        &tcp_row(LOOPBACK_6379, 1001),
        &tcp6_row(IPV6_LOOPBACK_6379, 1002),
        "",
    );

    // Act
    let discovered = only(&proc);

    // Assert
    assert_eq!(discovered.key, "6379");
    assert_eq!(discovered.kind, ServerKind::Redis);
    assert_eq!(listeners_of(&discovered), ["127.0.0.1:6379", "[::1]:6379"]);
    assert_eq!(
        discovered.reach,
        Ok(DialTarget::Tcp(
            "127.0.0.1:6379".parse().expect("an address")
        ))
    );
}

#[test]
fn a_unix_socket_is_reached_before_any_tcp_one() {
    // Arrange: Alpine's shipped configuration, which enables both.
    let proc = proc_with(
        "redis-discovery-unix-first",
        &[server("412", &[1001, 1003])],
        &tcp_row(LOOPBACK_6379, 1001),
        "",
        &unix_row("/run/redis/redis.sock", 1003),
    );

    // Act
    let discovered = only(&proc);

    // Assert: the key stays the port, so enabling the socket is a change to the instance
    // rather than one instance disappearing and another appearing.
    assert_eq!(discovered.key, "6379");
    assert_eq!(
        listeners_of(&discovered),
        ["127.0.0.1:6379", "/run/redis/redis.sock"]
    );
    assert_eq!(
        discovered.reach,
        Ok(DialTarget::Unix(PathBuf::from("/run/redis/redis.sock")))
    );
}

#[test]
fn a_server_on_a_unix_socket_alone_is_keyed_by_its_path() {
    // Arrange
    let proc = proc_with(
        "redis-discovery-unix-only",
        &[server("412", &[1003])],
        "",
        "",
        &unix_row("/run/redis/redis.sock", 1003),
    );

    // Act & Assert
    assert_eq!(only(&proc).key, "/run/redis/redis.sock");
}

#[test]
fn a_wildcard_listener_is_reached_on_loopback() {
    // Arrange: the field host's `bind 0.0.0.0`.
    let proc = proc_with(
        "redis-discovery-wildcard",
        &[server("412", &[1001])],
        &tcp_row(WILDCARD_6379, 1001),
        "",
        "",
    );

    // Act
    let discovered = only(&proc);

    // Assert: the document says what is bound, and the connection never leaves the box.
    assert_eq!(listeners_of(&discovered), ["0.0.0.0:6379"]);
    assert_eq!(
        discovered.reach,
        Ok(DialTarget::Tcp(
            "127.0.0.1:6379".parse().expect("an address")
        ))
    );
}

#[test]
fn an_ipv6_wildcard_listener_is_reached_on_ipv6_loopback() {
    // Arrange
    let proc = proc_with(
        "redis-discovery-wildcard6",
        &[server("412", &[1002])],
        "",
        &tcp6_row(IPV6_WILDCARD_6379, 1002),
        "",
    );

    // Act & Assert
    assert_eq!(
        only(&proc).reach,
        Ok(DialTarget::Tcp("[::1]:6379".parse().expect("an address")))
    );
}

#[test]
fn an_address_of_the_box_is_reached_as_it_is_bound() {
    // Arrange: bound to one interface's address and not to loopback at all. The kernel
    // delivers a connection to its own address locally, so this still never leaves the box.
    let proc = proc_with(
        "redis-discovery-own-address",
        &[server("412", &[1001])],
        &tcp_row(OWN_6379, 1001),
        "",
        "",
    );

    // Act & Assert
    assert_eq!(
        only(&proc).reach,
        Ok(DialTarget::Tcp(
            "10.0.0.5:6379".parse().expect("an address")
        ))
    );
}

#[test]
fn a_socket_another_process_holds_is_not_the_servers() {
    // Arrange: something else listens on 6380.
    let other = Process {
        pid: "900",
        comm: "haproxy",
        title: "haproxy\0",
        holds: Some(&[1009]),
    };
    let proc = proc_with(
        "redis-discovery-other-holder",
        &[server("412", &[1001]), other],
        &format!(
            "{}{}",
            tcp_row(LOOPBACK_6379, 1001),
            tcp_row(LOOPBACK_6380, 1009)
        ),
        "",
        "",
    );

    // Act & Assert
    assert_eq!(listeners_of(&only(&proc)), ["127.0.0.1:6379"]);
}

#[test]
fn a_server_whose_descriptors_are_unreadable_is_keyed_by_its_title_and_not_reached() {
    // Arrange: an unprivileged run, and a server owned by the `redis` account.
    let unreadable = Process {
        holds: None,
        ..server("412", &[])
    };
    let proc = proc_with(
        "redis-discovery-unreadable",
        &[unreadable],
        &tcp_row(LOOPBACK_6379, 1001),
        "",
        "",
    );

    // Act
    let discovered = only(&proc);

    // Assert: the title is what redis calls itself, readable by anybody, and a socket on the
    // box that nobody could be seen holding is not proof of whose it is.
    assert_eq!(discovered.key, "/usr/bin/redis-server 127.0.0.1:6379");
    assert!(discovered.listeners.is_empty());
    let refusal = discovered.reach.expect_err("an unreachable server");
    assert!(refusal.contains("descriptors"), "{refusal}");
}

#[test]
fn a_server_is_not_reached_where_the_socket_tables_are_unreadable() {
    // Arrange
    let proc = proc_with(
        "redis-discovery-no-tables",
        &[server("412", &[1001])],
        "",
        "",
        "",
    );
    for table in ["tcp", "tcp6", "unix"] {
        fs::remove_file(proc.join("net").join(table)).expect("a removable fixture");
    }

    // Act
    let refusal = only(&proc).reach.expect_err("an unreachable server");

    // Assert
    assert!(refusal.contains("socket tables"), "{refusal}");
}

#[test]
fn a_server_listening_on_nothing_says_so() {
    // Arrange: `port 0` and no `unixsocket`, which redis accepts.
    let proc = proc_with("redis-discovery-silent", &[server("412", &[])], "", "", "");

    // Act
    let discovered = only(&proc);

    // Assert
    assert_eq!(discovered.key, "/usr/bin/redis-server 127.0.0.1:6379");
    let refusal = discovered.reach.expect_err("an unreachable server");
    assert!(refusal.contains("listens on nothing"), "{refusal}");
}

#[test]
fn each_server_on_the_box_is_its_own_instance() {
    // Arrange: Debian's `redis-server@` template, two instances on two ports.
    let proc = proc_with(
        "redis-discovery-two",
        &[server("412", &[1001]), server("530", &[1004])],
        &format!(
            "{}{}",
            tcp_row(LOOPBACK_6379, 1001),
            tcp_row(LOOPBACK_6380, 1004)
        ),
        "",
        "",
    );

    // Act
    let keys: Vec<String> = discover(&proc)
        .into_iter()
        .map(|server| server.key)
        .collect();

    // Assert
    assert_eq!(keys, ["6379", "6380"]);
}

#[test]
fn servers_sharing_a_title_are_both_kept() {
    // Arrange: two unattributable servers whose titles agree, which a customised
    // `proc-title-template` makes easy.
    let first = Process {
        holds: None,
        title: "redis-server\0",
        ..server("412", &[])
    };
    let second = Process {
        holds: None,
        title: "redis-server\0",
        ..server("530", &[])
    };
    let proc = proc_with("redis-discovery-same-title", &[first, second], "", "", "");

    // Act
    let keys: Vec<String> = discover(&proc)
        .into_iter()
        .map(|server| server.key)
        .collect();

    // Assert: a map keyed by the title would have kept one and said nothing.
    assert_eq!(keys, ["redis-server", "redis-server #2"]);
}

#[test]
fn a_server_with_no_title_is_keyed_by_its_program() {
    // Arrange: an empty `cmdline`, which is what a process mid-exit leaves.
    let untitled = Process {
        holds: None,
        title: "",
        ..server("412", &[])
    };
    let proc = proc_with("redis-discovery-untitled", &[untitled], "", "", "");

    // Act & Assert: never an empty key, which would read as a value that went missing.
    assert_eq!(only(&proc).key, "redis-server");
}

#[test]
fn loopback_is_reached_before_an_address_of_the_box() {
    // Arrange: `bind 10.0.0.5 127.0.0.1`, where the box's own address sorts first.
    let proc = proc_with(
        "redis-discovery-loopback-first",
        &[server("412", &[1001, 1005])],
        &format!(
            "{}{}",
            tcp_row(OWN_6379, 1005),
            tcp_row(LOOPBACK_6379, 1001)
        ),
        "",
        "",
    );

    // Act & Assert
    assert_eq!(
        only(&proc).reach,
        Ok(DialTarget::Tcp(
            "127.0.0.1:6379".parse().expect("an address")
        ))
    );
}
