//! Which servers are on the box, what each is keyed by, and where rastro may reach it.
//!
//! Nothing here connects to anything. Discovery is `/proc` alone, so which socket a server is
//! reached on is decided from what the kernel says it holds, never from a guessed default.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::redis::{DialTarget, DiscoveredServer, ServerKind, discover};

mod support;

use support::fake_redis::HOST_NETWORK_NAMESPACE;
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
    /// The process that started it, as `/proc/<pid>/stat` names it.
    parent: &'a str,
}

fn server(pid: &'static str, holds: &'static [u64]) -> Process<'static> {
    Process {
        pid,
        comm: "redis-server",
        title: "/usr/bin/redis-server 127.0.0.1:6379\0",
        holds: Some(holds),
        parent: "1",
    }
}

/// A `/proc` with the processes and socket tables named.
fn proc_with(name: &str, processes: &[Process], tcp: &str, tcp6: &str, unix: &str) -> PathBuf {
    let proc = scratch_tree(name, &["proc/net", "proc/self/ns"]).join("proc");
    symlink(HOST_NETWORK_NAMESPACE, proc.join("self/ns/net")).expect("a writable scratch symlink");
    write(&proc, "net/tcp", &format!("{TCP_HEADER}{tcp}"));
    write(&proc, "net/tcp6", &format!("{TCP_HEADER}{tcp6}"));
    write(&proc, "net/unix", &format!("{UNIX_HEADER}{unix}"));

    for process in processes {
        let directory = proc.join(process.pid);
        fs::create_dir_all(&directory).expect("a writable scratch directory");
        // Arrange: on the host, in rastro's own network namespace.
        fs::create_dir_all(directory.join("ns")).expect("a writable scratch directory");
        symlink(HOST_NETWORK_NAMESPACE, directory.join("ns/net"))
            .expect("a writable scratch symlink");
        write(&directory, "comm", &format!("{}\n", process.comm));
        write(&directory, "cmdline", process.title);
        write(
            &directory,
            "stat",
            &format!(
                "{} ({}) S {} {} 0 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 0 0 0\n",
                process.pid, process.comm, process.parent, process.pid
            ),
        );

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
        parent: "1",
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
fn a_unix_socket_in_another_namespace_is_not_dialled_from_the_host() {
    // Arrange: a server in a container of its own, holding a unix socket and nothing else; its
    // path is its own root's, and the host's at that spelling is another socket or none.
    let proc = proc_with(
        "redis-discovery-container-unix",
        &[server("412", &[1003])],
        "",
        "",
        &unix_row("/run/redis/redis.sock", 1003),
    );
    fs::create_dir_all(proc.join("412/net")).expect("a writable scratch directory");
    for table in ["tcp", "tcp6", "unix"] {
        fs::copy(
            proc.join("net").join(table),
            proc.join("412/net").join(table),
        )
        .expect("a copied table");
    }
    fs::remove_file(proc.join("412/ns/net")).expect("a removable fixture");
    symlink("net:[4026532777]", proc.join("412/ns/net")).expect("a writable scratch symlink");

    // Act
    let discovered = only(&proc);

    // Assert: listed, and not reached.
    assert_eq!(listeners_of(&discovered), ["/run/redis/redis.sock"]);
    let refusal = discovered.reach.expect_err("an unreached server");
    assert!(refusal.contains("in its own namespace"), "{refusal}");
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

/// The key each server on one port gets, as which address it holds, with the older one first.
fn keys_on_one_port(name: &str, older: u64, younger: u64) -> Vec<(String, Vec<String>)> {
    let proc = proc_with(
        name,
        &[
            Process {
                holds: Some(&[older]),
                ..server("412", &[])
            },
            Process {
                holds: Some(&[younger]),
                ..server("530", &[])
            },
        ],
        &format!(
            "{}{}",
            tcp_row(LOOPBACK_6379, 1001),
            tcp_row(OWN_6379, 1002)
        ),
        "",
        "",
    );

    let mut keys: Vec<(String, Vec<String>)> = discover(&proc)
        .iter()
        .map(|server| (server.key.clone(), listeners_of(server)))
        .collect();
    keys.sort();
    keys
}

#[test]
fn servers_sharing_a_port_are_keyed_by_address_whichever_started_first() {
    // Act: the same two servers, before and after the older one restarts.
    let before = keys_on_one_port("redis-discovery-port-before", 1001, 1002);
    let after = keys_on_one_port("redis-discovery-port-after", 1002, 1001);

    // Assert: numbered by pid, a restart would swap them, which is a diff of nothing changed.
    assert_eq!(before, after);
    assert_eq!(
        before
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["10.0.0.5:6379", "127.0.0.1:6379"]
    );
}

/// Two containers each listening on `0.0.0.0:6379` in a namespace of its own, the older first.
fn keys_of_two_containers(name: &str, older: &str, younger: &str) -> Vec<(String, Vec<String>)> {
    let wildcard = tcp_row(WILDCARD_6379, 1001);
    let proc = proc_with(
        name,
        &[
            Process {
                holds: Some(&[1001]),
                ..server("412", &[])
            },
            Process {
                holds: Some(&[1001]),
                ..server("530", &[])
            },
        ],
        "",
        "",
        "",
    );
    for (pid, scope, namespace) in [
        ("412", older, "net:[4026532701]"),
        ("530", younger, "net:[4026532702]"),
    ] {
        fs::create_dir_all(proc.join(pid).join("net")).expect("a writable scratch directory");
        write(
            &proc.join(pid),
            "net/tcp",
            &format!("{TCP_HEADER}{wildcard}"),
        );
        write(&proc.join(pid), "net/tcp6", TCP_HEADER);
        write(&proc.join(pid), "net/unix", UNIX_HEADER);
        write(
            &proc.join(pid),
            "cgroup",
            &format!("0::/system.slice/{scope}\n"),
        );
        fs::remove_file(proc.join(pid).join("ns/net")).expect("a removable fixture");
        symlink(namespace, proc.join(pid).join("ns/net")).expect("a writable scratch symlink");
    }

    let mut keys: Vec<(String, Vec<String>)> = discover(&proc)
        .iter()
        .map(|server| {
            let cgroup =
                fs::read_to_string(proc.join(server.process_id.to_string()).join("cgroup"))
                    .expect("a cgroup");
            (server.key.clone(), vec![cgroup])
        })
        .collect();
    keys.sort();
    keys
}

#[test]
fn containers_sharing_an_address_are_keyed_by_their_scope_whichever_started_first() {
    // Act: the same two containers, before and after the older one restarts, which keeps its
    // scope and takes a new pid.
    let before = keys_of_two_containers(
        "redis-discovery-scope-before",
        "docker-aaa.scope",
        "docker-bbb.scope",
    );
    let after = keys_of_two_containers(
        "redis-discovery-scope-after",
        "docker-bbb.scope",
        "docker-aaa.scope",
    );

    // Assert: numbered by pid, a restart would swap them, which is a diff of nothing changed.
    assert_eq!(before, after);
    assert_eq!(
        before
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        [
            "0.0.0.0:6379 in docker-aaa.scope",
            "0.0.0.0:6379 in docker-bbb.scope"
        ]
    );
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

/// 6380, the plain port beside a TLS one on 6379.
const WILDCARD_6380: &str = "00000000:18EC";

#[test]
fn the_port_the_process_title_names_is_reached_before_one_that_sorts_first() {
    // Arrange: TLS on 6379, plain on 6380, both wildcard. Measured, the kernel's tables cannot
    // tell them apart, and redis's title names the plain port whenever there is one.
    let titled = Process {
        title: "/usr/bin/redis-server *:6380\0",
        ..server("412", &[1001, 1006])
    };
    let proc = proc_with(
        "redis-discovery-titled-port",
        &[titled],
        &format!(
            "{}{}",
            tcp_row(WILDCARD_6379, 1001),
            tcp_row(WILDCARD_6380, 1006)
        ),
        "",
        "",
    );

    // Act & Assert
    assert_eq!(
        only(&proc).reach,
        Ok(DialTarget::Tcp(
            "127.0.0.1:6380".parse().expect("an address")
        ))
    );
}

#[test]
fn within_the_titled_port_loopback_is_still_reached_first() {
    // Arrange: the title names the port and the box's own address, since it shows the first
    // `bind`; loopback on the same port is the more local choice.
    let titled = Process {
        title: "/usr/bin/redis-server 10.0.0.5:6379\0",
        ..server("412", &[1001, 1005])
    };
    let proc = proc_with(
        "redis-discovery-titled-loopback",
        &[titled],
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

#[test]
fn a_title_naming_no_port_leaves_the_usual_order() {
    // Arrange: a custom `proc-title-template` that leaves the address out.
    let untitled = Process {
        title: "redis-server [cache]\0",
        ..server("412", &[1001, 1006])
    };
    let proc = proc_with(
        "redis-discovery-untitled-port",
        &[untitled],
        &format!(
            "{}{}",
            tcp_row(WILDCARD_6379, 1001),
            tcp_row(WILDCARD_6380, 1006)
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

#[test]
fn a_child_a_server_forked_to_save_is_not_another_server() {
    // Arrange: measured on redis 5 to 8.10 and valkey 7.2 to 9.1, a background save or an AOF
    // rewrite forks a child that keeps `comm`, retitles itself `redis-rdb-bgsave *:6379`, and
    // closes the listeners. Counted as a server, it would come and go with the save.
    let saving = Process {
        pid: "413",
        title: "redis-rdb-bgsave *:6379\0",
        holds: Some(&[]),
        parent: "412",
        ..server("413", &[])
    };
    let proc = proc_with(
        "redis-discovery-bgsave",
        &[server("412", &[1001]), saving],
        &tcp_row(LOOPBACK_6379, 1001),
        "",
        "",
    );

    // Act
    let keys: Vec<String> = discover(&proc)
        .into_iter()
        .map(|server| server.key)
        .collect();

    // Assert
    assert_eq!(keys, ["6379"]);
}
