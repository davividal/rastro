//! The facet itself: what it says about a box, and what it declines to say.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use rastro::collectors::redis::{InstalledServers, RedisCollector, ServerKind};
use rastro_collector::{Collector, CollectorCategory, Presence};

mod support;

use support::captured_redis_cell::captured_reply;
use support::fake_redis::{
    FakeRedis, HOST_NETWORK_NAMESPACE, array_of, bulk, dead_socket, key_of, proc_holding,
    unknown_command,
};
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, is_null, items_of, keys_of, text};

/// A `/proc` holding the processes named, each as `(pid, comm, cmdline)`.
///
/// Arrange: `comm` is what identifies a server, because redis rewrites its own argument vector
/// into a process title the moment it has started and never touches `comm`. Measured on Debian
/// 12 and 13, Alpine, and the redis 5, redis 8 and valkey images.
fn proc_with(name: &str, processes: &[(&str, &str, &str)]) -> PathBuf {
    let proc = scratch_tree(name, &["proc/self/ns"]).join("proc");
    symlink(HOST_NETWORK_NAMESPACE, proc.join("self/ns/net")).expect("a writable scratch symlink");

    for (pid, comm, cmdline) in processes {
        // Arrange: on the host, in rastro's own network namespace.
        fs::create_dir_all(proc.join(pid).join("ns")).expect("a writable scratch directory");
        symlink(HOST_NETWORK_NAMESPACE, proc.join(pid).join("ns/net"))
            .expect("a writable scratch symlink");
        write(&proc, &format!("{pid}/comm"), &format!("{comm}\n"));
        write(&proc, &format!("{pid}/cmdline"), cmdline);
    }

    proc
}

fn collector(installed: &[ServerKind], proc: &Path) -> RedisCollector {
    RedisCollector::reading(InstalledServers::new(installed.iter().copied()), proc)
}

#[test]
fn the_facet_is_state_and_is_named_for_the_service() {
    // Arrange
    let proc = proc_with("redis-facet-name", &[]);

    // Act
    let collector = collector(&[], &proc);

    // Assert: state rather than metadata, so an operator may exclude it.
    assert_eq!(collector.name().as_str(), "redis");
    assert_eq!(collector.category(), CollectorCategory::State);
}

#[test]
fn presence_is_absent_where_no_server_is_installed_or_running() {
    // Arrange: a process table holding something else entirely.
    let proc = proc_with("redis-facet-absent", &[("1", "systemd", "/sbin/init\0")]);

    // Act & Assert
    assert_eq!(collector(&[], &proc).presence(), Presence::Absent);
}

#[test]
fn presence_is_present_where_a_server_is_installed_and_nothing_runs() {
    // Arrange
    let proc = proc_with("redis-facet-stopped", &[]);

    // Act & Assert: installed and stopped is a different fact from never installed.
    assert_eq!(
        collector(&[ServerKind::Redis], &proc).presence(),
        Presence::Present
    );
}

#[test]
fn presence_is_present_where_a_server_runs_from_no_system_directory() {
    // Arrange: a valkey built into `/opt`, which no located binary accounts for.
    let proc = proc_with(
        "redis-facet-running",
        &[(
            "812",
            "valkey-server",
            "/opt/valkey/bin/valkey-server *:6379\0",
        )],
    );

    // Act & Assert: a running server is not hidden by a binary rastro did not find.
    assert_eq!(collector(&[], &proc).presence(), Presence::Present);
}

#[test]
fn collect_names_the_servers_installed_on_a_box_where_nothing_runs() {
    // Arrange: both families, which Debian 13 and Alpine package side by side.
    let proc = proc_with("redis-facet-installed", &[]);

    // Act
    let observation = collector(&[ServerKind::Valkey, ServerKind::Redis], &proc)
        .collect()
        .expect("a box with nothing running is readable");

    // Assert: sorted, so the order binaries were located in cannot move a diff.
    let installed: Vec<String> = items_of(&field(&observation, "installed"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(installed, ["redis", "valkey"]);
}

/// The one instance a single-server fixture holds.
fn instance_of(
    observation: &rastro_collector::Observation,
    key: &str,
) -> rastro_collector::Observation {
    let instances = field(observation, "instances");
    assert_eq!(keys_of(&instances), [key]);

    field(&instances, key)
}

#[test]
fn a_server_that_answers_is_an_instance_with_nothing_missing() {
    // Arrange
    let server = FakeRedis::stock("facet-answers", &[]);
    let proc = server.proc("redis-facet-answers");

    // Act
    let observation = collector(&[ServerKind::Redis], &proc)
        .collect()
        .expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert_eq!(text(&field(&instance, "server")), "redis");
    assert_eq!(text(&field(&instance, "version")), "8.10.2");
    assert_eq!(text(&field(&instance, "mode")), "standalone");
    assert_eq!(
        text(&field(&instance, "executable")),
        "/usr/bin/redis-server"
    );
    assert_eq!(
        text(&field(&instance, "config_file")),
        "/etc/redis/redis.conf"
    );
    let listening: Vec<String> = items_of(&field(&instance, "listening"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(listening, [key_of(&server)]);
    assert!(is_null(&field(&instance, "error")));
}

#[test]
fn a_supported_server_is_not_marked() {
    // Arrange: redis 8.10, the redis.io package's own reply.
    let server = FakeRedis::stock("facet-supported", &[]);
    let proc = server.proc("redis-facet-supported");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert!(is_null(&field(&instance, "unsupported")));
    assert_eq!(observation.approximate_items(), 0);
}

#[test]
fn an_older_server_is_read_in_full_and_marked_unsupported() {
    // Arrange: redis 7.4, which endoflife.date lists as maintained and the maintainer left out.
    let info = captured_reply("33", "INFO server");
    let server = FakeRedis::stock("facet-unsupported", &[("INFO server", &info)]);
    let proc = server.proc("redis-facet-unsupported");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: read, and marked, so the run's summary tells the operator.
    let instance = instance_of(&observation, &key_of(&server));
    assert_eq!(text(&field(&instance, "version")), "7.4.11");
    assert_eq!(
        text(&field(&instance, "unsupported")),
        "redis 7.4.11 is not a release rastro supports, so it is read on a best-effort basis"
    );
    assert!(!is_null(&field(&instance, "settings")));
    assert!(is_null(&field(&instance, "error")));
    assert_eq!(observation.approximate_items(), 1);
}

#[test]
fn a_server_that_wants_a_password_is_an_instance_with_an_error() {
    // Arrange
    let server = FakeRedis::answering(
        "facet-noauth",
        &[("INFO server", "-NOAUTH Authentication required.\r\n")],
    );
    let proc = server.proc("redis-facet-noauth");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: the refusal is on the instance, and the facet around it is still a reading.
    let instance = instance_of(&observation, &key_of(&server));
    let error = text(&field(&instance, "error"));
    assert!(error.contains("password"), "{error}");
    assert_eq!(observation.incomplete_items(), 1);
}

#[test]
fn a_server_whose_socket_refuses_is_an_instance_with_an_error() {
    // Arrange: the kernel lists the socket and nothing accepts on it.
    let socket = dead_socket("facet-refuses");
    let proc = proc_holding("redis-facet-refuses", &socket);

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &socket.display().to_string());
    let error = text(&field(&instance, "error"));
    assert!(error.contains("could not connect"), "{error}");
    let _ = fs::remove_file(socket);
}

#[test]
fn a_server_that_was_not_reached_is_an_instance_with_the_reason() {
    // Arrange: descriptors this run may not read.
    let proc = proc_with(
        "redis-facet-unreached",
        &[(
            "412",
            "redis-server",
            "/usr/bin/redis-server 127.0.0.1:6379\0",
        )],
    );
    write(&proc, "412/fd", "");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, "/usr/bin/redis-server 127.0.0.1:6379");
    let error = text(&field(&instance, "error"));
    assert!(error.contains("descriptors"), "{error}");
    assert_eq!(items_of(&field(&instance, "listening")).len(), 0);
}

#[test]
fn a_valkey_answering_under_the_redis_name_is_reported_as_valkey() {
    // Arrange: Debian's valkey compatibility package installs a `redis-server` symlink, so the
    // kernel's `comm` says redis about a server that is not.
    let info = bulk(
        "# Server\r\nredis_version:7.2.4\r\nserver_name:valkey\r\nvalkey_version:8.1.1\r\nserver_mode:standalone\r\n",
    );
    let server = FakeRedis::stock("facet-valkey", &[("INFO server", &info)]);
    let proc = server.proc("redis-facet-valkey");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: once connected, the server's own account of itself wins.
    let instance = instance_of(&observation, &key_of(&server));
    assert_eq!(text(&field(&instance, "server")), "valkey");
    assert_eq!(text(&field(&instance, "version")), "8.1.1");
}

#[test]
fn a_server_that_was_not_asked_has_no_version() {
    // Arrange
    let server = FakeRedis::answering(
        "facet-unasked",
        &[("INFO server", "-NOAUTH Authentication required.\r\n")],
    );
    let proc = server.proc("redis-facet-unasked");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: null, never a guessed version, and the family from `comm` still stands.
    let instance = instance_of(&observation, &key_of(&server));
    assert_eq!(text(&field(&instance, "server")), "redis");
    assert!(is_null(&field(&instance, "version")));
}

#[test]
fn an_instance_carries_the_settings_the_server_is_running_with() {
    // Arrange: `maxmemory` and `save` applied with `CONFIG SET` and in no file, which is the
    // estate's arrangement and the reason this facet asks the server.
    let config = array_of(&[
        "maxmemory",
        "32212254720",
        "save",
        "",
        "requirepass",
        "hunter2",
    ]);
    let server = FakeRedis::stock("facet-settings", &[("CONFIG GET *", &config)]);
    let proc = server.proc("redis-facet-settings");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let settings = field(&instance_of(&observation, &key_of(&server)), "settings");
    assert_eq!(text(&field(&settings, "maxmemory")), "32212254720");
    assert_eq!(text(&field(&settings, "save")), "");
    assert_eq!(
        field(&settings, "requirepass").sensitivity(),
        rastro_fingerprint::Sensitivity::Sensitive
    );
}

#[test]
fn a_server_with_config_renamed_away_keeps_everything_else() {
    // Arrange: `rename-command CONFIG ""`, common hardening. Measured: `ERR unknown command`,
    // and `INFO` still answers.
    let refused = unknown_command("CONFIG");
    let server = FakeRedis::stock("facet-no-config", &[("CONFIG GET *", &refused)]);
    let proc = server.proc("redis-facet-no-config");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: the refused read is null and says why; what was read stays.
    let instance = instance_of(&observation, &key_of(&server));
    assert!(is_null(&field(&instance, "settings")));
    let error = text(&field(&instance, "error"));
    assert!(error.contains("CONFIG GET"), "{error}");
    assert_eq!(text(&field(&instance, "version")), "8.10.2");
}

#[test]
fn a_server_that_wants_a_password_is_asked_nothing_more() {
    // Arrange
    let server = FakeRedis::stock(
        "facet-noauth-quiet",
        &[("INFO server", "-NOAUTH Authentication required.\r\n")],
    );
    let proc = server.proc("redis-facet-noauth-quiet");

    // Act
    collector(&[], &proc).collect().expect("a readable box");

    // Assert: every command sent is one more thing a server logs or counts.
    assert_eq!(server.received(), [["INFO", "server"]]);
}

#[test]
fn an_instance_says_what_it_replicates() {
    // Arrange
    let replication = bulk(
        "# Replication\r\nrole:slave\r\nmaster_host:10.0.0.1\r\nmaster_port:6379\r\nmaster_link_status:up\r\n",
    );
    let server = FakeRedis::stock("facet-replica", &[("INFO replication", &replication)]);
    let proc = server.proc("redis-facet-replica");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let replication = field(&instance_of(&observation, &key_of(&server)), "replication");
    assert_eq!(text(&field(&replication, "role")), "slave");
    assert_eq!(text(&field(&replication, "master")), "10.0.0.1:6379");
}

#[test]
fn a_refused_replication_read_costs_only_itself() {
    // Arrange
    let refused = unknown_command("INFO");
    let server = FakeRedis::stock("facet-no-replication", &[("INFO replication", &refused)]);
    let proc = server.proc("redis-facet-no-replication");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert!(is_null(&field(&instance, "replication")));
    assert!(text(&field(&instance, "error")).contains("INFO replication"));
    // Every setting redis 8.10's package answers with, read in full.
    assert_eq!(keys_of(&field(&instance, "settings")).len(), 303);
}

#[test]
fn an_instance_lists_its_accounts() {
    // Arrange
    let server = FakeRedis::stock("facet-acl", &[]);
    let proc = server.proc("redis-facet-acl");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let acl = field(&instance_of(&observation, &key_of(&server)), "acl");
    assert_eq!(keys_of(&acl), ["default"]);
}

#[test]
fn a_server_older_than_accounts_is_not_asked_for_them() {
    // Arrange: the field host's 5.0, where `ACL` does not exist. Measured on 5.0.14.
    let info = bulk("# Server\r\nredis_version:5.0.3\r\nredis_mode:standalone\r\n");
    let server = FakeRedis::stock("facet-acl-old", &[("INFO server", &info)]);
    let proc = server.proc("redis-facet-acl-old");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: absent rather than refused, so no error, and nothing sent to find out.
    let instance = instance_of(&observation, &key_of(&server));
    assert!(is_null(&field(&instance, "acl")));
    assert!(is_null(&field(&instance, "error")));
    assert!(!server.received().iter().any(|words| words[0] == "ACL"));
}

#[test]
fn a_refused_account_list_says_so() {
    // Arrange: an account allowed to read everything but its own kind.
    let server = FakeRedis::stock(
        "facet-acl-noperm",
        &[(
            "ACL LIST",
            "-NOPERM User default has no permissions to run the 'acl|list' command\r\n",
        )],
    );
    let proc = server.proc("redis-facet-acl-noperm");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert!(is_null(&field(&instance, "acl")));
    assert!(text(&field(&instance, "error")).contains("ACL LIST"));
}

#[test]
fn an_instance_lists_the_modules_it_loaded() {
    // Arrange
    let modules = "*1\r\n*4\r\n$4\r\nname\r\n$6\r\nReJSON\r\n$3\r\nver\r\n:20609\r\n";
    let server = FakeRedis::stock("facet-modules", &[("MODULE LIST", modules)]);
    let proc = server.proc("redis-facet-modules");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let modules = field(&instance_of(&observation, &key_of(&server)), "modules");
    assert_eq!(keys_of(&modules), ["ReJSON"]);
}

#[test]
fn a_refused_module_list_says_so() {
    // Arrange
    let refused = unknown_command("MODULE");
    let server = FakeRedis::stock("facet-no-modules", &[("MODULE LIST", &refused)]);
    let proc = server.proc("redis-facet-no-modules");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert!(is_null(&field(&instance, "modules")));
    assert!(text(&field(&instance, "error")).contains("MODULE LIST"));
}

#[test]
fn a_socket_that_hangs_up_names_the_sockets_not_tried_and_why() {
    // Arrange: a listener that closes every connection unanswered, which is what a TLS port
    // does to a plain client, and a second socket the server holds on the box's own address.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
        }
    });
    let header = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
    let row = |address: &str, inode: u64| {
        format!(
            "   0: {address} 00000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
        )
    };
    let proc = proc_with(
        "redis-facet-hangs-up",
        &[("412", "redis-server", "redis-server [cache]\0")],
    );
    write(
        &proc,
        "net/tcp",
        &format!(
            "{header}{}{}",
            row(&format!("0100007F:{port:04X}"), 2001),
            row("0500000A:18EC", 2002)
        ),
    );
    write(&proc, "net/tcp6", header);
    write(
        &proc,
        "net/unix",
        "Num       RefCount Protocol Flags    Type St Inode Path\n",
    );
    fs::create_dir_all(proc.join("412/fd")).expect("a writable scratch directory");
    std::os::unix::fs::symlink("socket:[2001]", proc.join("412/fd/3")).expect("a symlink");
    std::os::unix::fs::symlink("socket:[2002]", proc.join("412/fd/4")).expect("a symlink");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: the other socket is named, and it was not tried, since a plain client on a TLS
    // port is a line in the server's log.
    // Keyed by its lowest port, which is the one it was not reached on.
    let instance = instance_of(&observation, "6380");
    let error = text(&field(&instance, "error"));
    assert!(error.contains("10.0.0.5:6380"), "{error}");
    assert!(error.contains("not tried"), "{error}");
    assert!(!error.contains(&format!("127.0.0.1:{port}")), "{error}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_socket_the_kernel_says_another_process_holds_is_sent_nothing() {
    // Arrange: `/proc/net/unix` prints a socket path raw, newline included, measured, so another
    // account can bind a path that forges a row naming the server's inode. Here the table says
    // the server holds the socket, and the kernel, asked about the connected peer, says otherwise.
    let server = FakeRedis::stock_with_password("facet-not-the-server", "hunter2", &[]);
    let proc =
        support::fake_redis::proc_holding_as("redis-facet-not-the-server", &server.socket, "412");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert: refused before a single byte, so nothing, a password least of all, reaches it.
    let instance = instance_of(&observation, &key_of(&server));
    let error = text(&field(&instance, "error"));
    assert!(error.contains("not the server's own"), "{error}");
    assert!(server.received().is_empty(), "{:?}", server.received());
}

#[test]
fn a_sentinel_is_named_and_asked_nothing_more() {
    // Arrange: `redis-server --sentinel` keeps `comm`, and measured on redis 8.10 it rejects
    // `CONFIG GET *` and `MODULE LIST`; each rejection is one more error the server counts.
    let info = bulk("# Server\r\nredis_version:8.10.2\r\nredis_mode:sentinel\r\n");
    let server = FakeRedis::stock("facet-sentinel", &[("INFO server", &info)]);
    let proc = server.proc("redis-facet-sentinel");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert_eq!(text(&field(&instance, "mode")), "sentinel");
    assert!(text(&field(&instance, "error")).contains("sentinel"));
    assert_eq!(server.received(), [["INFO", "server"]]);
}

#[test]
fn a_long_refusal_from_the_server_is_shortened() {
    // Arrange: the server's own words are the redis account's to choose, at any length.
    let refusal = format!("-ERR {}\r\n", "x".repeat(5_000));
    let server = FakeRedis::stock("facet-long-refusal", &[("CONFIG GET *", &refusal)]);
    let proc = server.proc("redis-facet-long-refusal");

    // Act
    let observation = collector(&[], &proc).collect().expect("a readable box");

    // Assert
    let error = text(&field(
        &instance_of(&observation, &key_of(&server)),
        "error",
    ));
    assert!(error.len() < 400, "{}", error.len());
    assert!(error.contains('…'), "{error}");
}
