//! Where a containerd on this box is listening, and where it keeps what it holds.
//!
//! Its own test file, because the discovery has several paths and two traps, and none of
//! them needs a facet or an engine to exercise.

mod support;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use rastro::collectors::containers::ContainerdLayout;
use support::fs_tree::scratch_tree;

/// A `/proc` holding one process, with the executable and command line given.
///
/// The executable is a symlink to a path that need not exist, which is what `/proc/<pid>/exe`
/// is: a link to a binary that may since have been replaced or deleted.
fn proc_with(name: &str, executable: &str, arguments: &[&str]) -> std::path::PathBuf {
    let root = scratch_tree(&format!("containerd-proc-{name}"), &["1234", "7", "self"]);

    symlink(executable, root.join("1234/exe")).expect("a writable scratch link");
    fs::write(root.join("1234/cmdline"), arguments.join("\0")).expect("a writable fixture");

    // A second process, so the search has something to reject.
    symlink("/usr/sbin/sshd", root.join("7/exe")).expect("a writable scratch link");
    fs::write(root.join("7/cmdline"), "/usr/sbin/sshd\0-D").expect("a writable fixture");

    root
}

/// A containerd configuration holding both addresses it can hold, and its two directories.
///
/// **The trap this file exists for.** The first `address =` in a real containerd config is
/// the *debug* socket, and the one `ctr` needs is under `[grpc]` further down. Measured on
/// containerd 2.3.4 as docker 29 ships it. Anything that took the first match would talk to
/// the debug endpoint, which answers a different API.
fn config_naming(root: &Path, grpc: &str) -> String {
    config_holding(root, grpc, "", "")
}

/// The same, naming the two directories containerd keeps its own state in.
///
/// The spellings are docker's own, measured on 26.1.5 and 29.8.0 alike: its managed
/// containerd is given `root = "/var/lib/docker/containerd/daemon"` and
/// `state = "/var/run/docker/containerd/daemon"`.
fn config_holding(root: &Path, grpc: &str, own_root: &str, own_state: &str) -> String {
    let path = root.join("containerd.toml");
    let directories = match (own_root.is_empty(), own_state.is_empty()) {
        (true, true) => String::new(),
        _ => format!("root = '{own_root}'\nstate = '{own_state}'\n"),
    };
    fs::write(
        &path,
        format!(
            "version = 3\n{directories}\n[debug]\n  address = '/run/containerd/debug.sock'\n  \
             level = ''\n\n[cgroup]\n  path = ''\n\n[grpc]\n  address = '{grpc}'\n  \
             max_recv_message_size = 16777216\n"
        ),
    )
    .expect("a writable fixture");

    path.to_str().expect("a UTF-8 scratch path").to_owned()
}

#[test]
fn the_address_comes_from_the_running_containerds_own_flag() {
    // Arrange
    let proc = proc_with(
        "flag",
        "/usr/bin/containerd",
        &[
            "/usr/bin/containerd",
            "--address",
            "/run/mine/containerd.sock",
        ],
    );

    // Act
    let address = ContainerdLayout::under(&proc).address;

    // Assert
    assert_eq!(
        address.map(|address| address.as_str().to_owned()),
        Some("/run/mine/containerd.sock".to_owned())
    );
}

#[test]
fn the_flag_is_read_when_it_is_written_with_an_equals_sign() {
    // Arrange: both spellings are legal on the command line, and a process started by a
    // unit file is as likely to carry one as the other.
    let proc = proc_with(
        "equals",
        "/usr/bin/containerd",
        &["/usr/bin/containerd", "--address=/run/mine/containerd.sock"],
    );

    // Act & Assert
    assert_eq!(
        ContainerdLayout::under(&proc)
            .address
            .map(|address| address.as_str().to_owned()),
        Some("/run/mine/containerd.sock".to_owned())
    );
}

#[test]
fn the_address_comes_from_the_configuration_the_process_names() {
    // Arrange: this is what a docker box looks like. Measured on docker 29.8.0: containerd
    // runs as `containerd --config /var/run/docker/containerd/containerd.toml` with no
    // address flag at all, and the socket is only in that file.
    let root = scratch_tree("containerd-config-named", &[]);
    let config = config_naming(&root, "/var/run/docker/containerd/containerd.sock");
    let proc = proc_with(
        "config",
        "/usr/local/bin/containerd",
        &["/usr/local/bin/containerd", "--config", &config],
    );

    // Act & Assert
    assert_eq!(
        ContainerdLayout::under(&proc)
            .address
            .map(|address| address.as_str().to_owned()),
        Some("/var/run/docker/containerd/containerd.sock".to_owned())
    );
}

#[test]
fn the_debug_socket_is_not_mistaken_for_the_one_that_answers() {
    // Arrange: the whole reason the configuration is parsed rather than searched. `[debug]`
    // comes first in the file docker's containerd is given.
    let root = scratch_tree("containerd-debug-trap", &[]);
    let config = config_naming(&root, "/run/real/containerd.sock");
    let proc = proc_with(
        "debug",
        "/usr/bin/containerd",
        &["/usr/bin/containerd", "--config", &config],
    );

    // Act
    let address = ContainerdLayout::under(&proc).address.expect("an address");

    // Assert
    assert_eq!(address.as_str(), "/run/real/containerd.sock");
    assert_ne!(address.as_str(), "/run/containerd/debug.sock");
}

#[test]
fn a_containerd_that_names_nothing_is_at_the_documented_default() {
    // Arrange: a containerd started with no arguments listens where containerd documents,
    // and reporting nothing would lose an engine that is plainly there.
    let proc = proc_with("bare", "/usr/bin/containerd", &["/usr/bin/containerd"]);

    // Act & Assert
    assert_eq!(
        ContainerdLayout::under(&proc)
            .address
            .map(|address| address.as_str().to_owned()),
        Some("/run/containerd/containerd.sock".to_owned())
    );
}

#[test]
fn a_box_with_no_containerd_running_has_no_address() {
    // Arrange: nothing to discover, and the default is not a guess worth making when no
    // containerd is running to be behind it.
    let proc = proc_with("absent", "/usr/sbin/nginx", &["nginx", "-g", "daemon off;"]);

    // Act & Assert
    assert!(ContainerdLayout::under(&proc).address.is_none());
}

#[test]
fn a_configuration_that_cannot_be_read_falls_back_to_the_default() {
    // Arrange: the process is there and names a file rastro cannot read, which an
    // unprivileged run makes ordinary. The engine is running, so the documented address is
    // a better answer than none, and `ctr` will say so loudly if it is the wrong one.
    let proc = proc_with(
        "unreadable-config",
        "/usr/bin/containerd",
        &[
            "/usr/bin/containerd",
            "--config",
            "/nowhere/containerd.toml",
        ],
    );

    // Act & Assert
    assert_eq!(
        ContainerdLayout::under(&proc)
            .address
            .map(|address| address.as_str().to_owned()),
        Some("/run/containerd/containerd.sock".to_owned())
    );
}

#[test]
fn the_directories_come_from_the_configuration_too() {
    // Arrange: measured on docker 26.1.5 and 29.8.0 alike, its managed containerd is given
    // both, and neither is where a standalone containerd would keep them.
    let root = scratch_tree("containerd-directories", &["store", "runtime"]);
    let store = root.join("store");
    let runtime = root.join("runtime");
    let config = config_holding(
        &root,
        "/run/mine/containerd.sock",
        store.to_str().expect("utf-8"),
        runtime.to_str().expect("utf-8"),
    );
    let proc = proc_with(
        "directories",
        "/usr/bin/containerd",
        &["/usr/bin/containerd", "--config", &config],
    );

    // Act
    let layout = ContainerdLayout::under(&proc);

    // Assert
    assert_eq!(
        layout.root.map(|root| root.as_str().to_owned()),
        Some(store.to_str().expect("utf-8").to_owned())
    );
    assert_eq!(
        layout.state.map(|state| state.as_str().to_owned()),
        Some(runtime.to_str().expect("utf-8").to_owned())
    );
}

#[test]
fn a_containerd_naming_no_directories_is_at_the_documented_defaults() {
    // Arrange: a containerd started with nothing keeps its store and its runtime state
    // where containerd documents, and those are the trees worth claiming on such a box.
    let proc = proc_with(
        "default-dirs",
        "/usr/bin/containerd",
        &["/usr/bin/containerd"],
    );

    // Act
    let layout = ContainerdLayout::under(&proc);

    // Assert
    assert_eq!(
        layout.root.map(|root| root.as_str().to_owned()),
        Some("/var/lib/containerd".to_owned())
    );
    assert_eq!(
        layout.state.map(|state| state.as_str().to_owned()),
        Some("/run/containerd".to_owned())
    );
}

#[test]
fn the_address_is_recorded_as_the_engine_reported_it() {
    // Arrange: **not resolved, unlike the directories.** docker names its containerd's
    // socket under `/var/run`, which on Linux is a symlink to `/run`; the address is handed
    // to `ctr`, which follows it, and what rastro records is where it asked. Resolving it
    // would also make this assertion depend on whether the box running the test happens to
    // have that symlink, which is how a test passes on macOS and fails on Debian.
    let root = scratch_tree("containerd-address-verbatim", &[]);
    let config = config_naming(&root, "/var/run/docker/containerd/containerd.sock");
    let proc = proc_with(
        "verbatim",
        "/usr/local/bin/containerd",
        &["/usr/local/bin/containerd", "--config", &config],
    );

    // Act & Assert
    assert_eq!(
        ContainerdLayout::under(&proc)
            .address
            .map(|address| address.as_str().to_owned()),
        Some("/var/run/docker/containerd/containerd.sock".to_owned())
    );
}

#[test]
fn a_directory_reached_through_a_symlink_is_recorded_as_the_walk_would_see_it() {
    // Arrange: **the second trap, and it would have made the claim do nothing.** docker's
    // containerd is given `state = "/var/run/docker/containerd/daemon"`, and on Debian
    // `/var/run` is a symlink to `/run`. The filesystem walk never follows a symlink, so it
    // only ever sees the real path; a claim naming the symlinked one is a rule about a tree
    // nothing visits.
    let root = scratch_tree("containerd-symlink", &["run/containerd"]);
    symlink("run", root.join("var-run")).expect("a writable scratch link");
    let through_the_link = root.join("var-run/containerd");
    let config = config_holding(
        &root,
        "/run/mine/containerd.sock",
        through_the_link.to_str().expect("utf-8"),
        "",
    );
    let proc = proc_with(
        "symlink",
        "/usr/bin/containerd",
        &["/usr/bin/containerd", "--config", &config],
    );

    // Act
    let layout = ContainerdLayout::under(&proc);

    // Assert
    let recorded = layout.root.expect("a root").as_str().to_owned();
    assert!(
        recorded.ends_with("/run/containerd"),
        "the symlink should be resolved to what the walk sees, got {recorded:?}"
    );
    assert!(!recorded.contains("var-run"));
}
