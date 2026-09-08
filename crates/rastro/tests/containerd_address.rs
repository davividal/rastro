//! Where a containerd on this box is listening.
//!
//! Its own test file, because the discovery has four paths and one trap, and none of them
//! needs a facet or an engine to exercise.

mod support;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use rastro::collectors::containers::ContainerdAddress;
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

/// A containerd configuration holding both addresses it can hold.
///
/// **The trap this file exists for.** The first `address =` in a real containerd config is
/// the *debug* socket, and the one `ctr` needs is under `[grpc]` further down. Measured on
/// containerd 2.3.4 as docker 29 ships it. Anything that took the first match would talk to
/// the debug endpoint, which answers a different API.
fn config_naming(root: &Path, grpc: &str) -> String {
    let path = root.join("containerd.toml");
    fs::write(
        &path,
        format!(
            "version = 3\n\n[debug]\n  address = '/run/containerd/debug.sock'\n  level = ''\n\n\
             [cgroup]\n  path = ''\n\n[grpc]\n  address = '{grpc}'\n  \
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
    let address = ContainerdAddress::under(&proc);

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
        ContainerdAddress::under(&proc).map(|address| address.as_str().to_owned()),
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
        ContainerdAddress::under(&proc).map(|address| address.as_str().to_owned()),
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
    let address = ContainerdAddress::under(&proc).expect("an address");

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
        ContainerdAddress::under(&proc).map(|address| address.as_str().to_owned()),
        Some("/run/containerd/containerd.sock".to_owned())
    );
}

#[test]
fn a_box_with_no_containerd_running_has_no_address() {
    // Arrange: nothing to discover, and the default is not a guess worth making when no
    // containerd is running to be behind it.
    let proc = proc_with("absent", "/usr/sbin/nginx", &["nginx", "-g", "daemon off;"]);

    // Act & Assert
    assert!(ContainerdAddress::under(&proc).is_none());
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
        ContainerdAddress::under(&proc).map(|address| address.as_str().to_owned()),
        Some("/run/containerd/containerd.sock".to_owned())
    );
}
