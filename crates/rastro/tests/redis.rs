//! The facet itself: what it says about a box, and what it declines to say.

use std::fs;
use std::path::{Path, PathBuf};

use rastro::collectors::redis::{InstalledServers, RedisCollector, ServerKind};
use rastro_collector::{Collector, CollectorCategory, Presence};

mod support;

use support::fs_tree::{scratch_tree, write};
use support::observation::{field, items_of, text};

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
