//! The facet itself: what it says about a box, and what it declines to say.

use std::fs;
use std::path::{Path, PathBuf};

use rastro::collectors::redis::{InstalledServers, RedisCollector, ServerKind};
use rastro_collector::{Collector, CollectorCategory, Presence};

mod support;

use support::fake_redis::{FakeRedis, dead_socket, key_of, proc_holding};
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, is_null, items_of, keys_of, text};

/// A `/proc` holding the processes named, each as `(pid, comm, cmdline)`.
///
/// Arrange: `comm` is what identifies a server, because redis rewrites its own argument vector
/// into a process title the moment it has started and never touches `comm`. Measured on Debian
/// 12 and 13, Alpine, and the redis 5, redis 8 and valkey images.
fn proc_with(name: &str, processes: &[(&str, &str, &str)]) -> PathBuf {
    let proc = scratch_tree(name, &["proc"]).join("proc");

    for (pid, comm, cmdline) in processes {
        fs::create_dir_all(proc.join(pid)).expect("a writable scratch directory");
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
    let server = FakeRedis::answering("facet-answers", &[("PING", "+PONG\r\n")]);
    let proc = server.proc("redis-facet-answers");

    // Act
    let observation = collector(&[ServerKind::Redis], &proc)
        .collect()
        .expect("a readable box");

    // Assert
    let instance = instance_of(&observation, &key_of(&server));
    assert_eq!(text(&field(&instance, "server")), "redis");
    let listening: Vec<String> = items_of(&field(&instance, "listening"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(listening, [key_of(&server)]);
    assert!(is_null(&field(&instance, "error")));
}

#[test]
fn a_server_that_wants_a_password_is_an_instance_with_an_error() {
    // Arrange
    let server = FakeRedis::answering(
        "facet-noauth",
        &[("PING", "-NOAUTH Authentication required.\r\n")],
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
