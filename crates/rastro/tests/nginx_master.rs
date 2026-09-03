//! Finding the nginx that is running, and what it was told to read.
//!
//! Everything asserted here was measured on a real master first: nginx rewrites its argument
//! vector into a process title, `/proc/<pid>`'s mtime is the moment the process began, and a
//! reload leaves the master's untouched while every worker gets a new one. The fixture below
//! is a `/proc` shaped like the one that behaviour produces.

use std::fs;
use std::os::unix::fs::{MetadataExt, symlink};
use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::nginx::master_process;
use support::fs_tree::{scratch_tree, write};

/// The title a master carries, NUL-separated the way the kernel returns it.
const MASTER: &str = "nginx: master process /usr/sbin/nginx\0-c\0/etc/nginx/other.conf\0";

/// A worker's title, which nginx pads with spaces.
const WORKER: &str = "nginx: worker process      \0";

fn proc_tree(name: &str, master_title: &str) -> PathBuf {
    let root = scratch_tree(&format!("nginx-master-{name}"), &["4", "5", "6", "7"]);
    let binary = root.join("nginx");
    write(&root, "nginx", "#!/bin/false\n");

    write(&root, "4/comm", "nginx\n");
    write(&root, "4/cmdline", master_title);
    symlink(&binary, root.join("4/exe")).expect("a writable scratch tree");

    for worker in ["5", "6"] {
        write(&root, &format!("{worker}/comm"), "nginx\n");
        write(&root, &format!("{worker}/cmdline"), WORKER);
        symlink(&binary, root.join(worker).join("exe")).expect("a writable scratch tree");
    }

    // A process that is not nginx at all, to prove the scan does not take everything it finds.
    write(&root, "7/comm", "sshd\n");
    write(&root, "7/cmdline", "/usr/sbin/sshd\0-D\0");

    root
}

fn started(path: &Path) -> i64 {
    fs::metadata(path)
        .expect("the fixture directory is there")
        .mtime()
}

#[test]
fn the_master_is_the_process_whose_title_says_so() {
    // Arrange
    let proc = proc_tree("found", MASTER);

    // Act
    let master = master_process::find_in(&proc, &proc.join("nginx"))
        .expect("the fixture is readable")
        .expect("the fixture holds a master");

    // Assert
    assert_eq!(master.process_id, 4);
    assert_eq!(
        master
            .executable
            .expect("this fixture's exe link is readable")
            .as_str(),
        proc.join("nginx").to_str().unwrap()
    );
    assert_eq!(master.started_at.as_i64(), started(&proc.join("4")));
}

#[test]
fn the_masters_command_line_says_which_configuration_is_being_served() {
    // Arrange: a master started with `-c`, which is a different file from the one the binary
    // was built to read.
    let proc = proc_tree("flags", MASTER);

    // Act
    let master = master_process::find_in(&proc, &proc.join("nginx"))
        .expect("the fixture is readable")
        .expect("the fixture holds a master");

    // Assert
    assert_eq!(
        master
            .configuration_path
            .expect("this master was given a -c")
            .as_str(),
        "/etc/nginx/other.conf"
    );
    assert_eq!(master.prefix, None);
}

#[test]
fn a_value_glued_to_its_flag_is_read_too() {
    // Arrange: nginx accepts `-c/path` as well as `-c /path`, so a grammar that only
    // compared whole tokens would report the wrong configuration for the second spelling.
    let proc = proc_tree(
        "glued",
        "nginx: master process /usr/sbin/nginx\0-c/etc/nginx/glued.conf\0-p/srv/nginx\0",
    );

    // Act
    let master = master_process::find_in(&proc, &proc.join("nginx"))
        .expect("the fixture is readable")
        .expect("the fixture holds a master");

    // Assert
    assert_eq!(
        master
            .configuration_path
            .expect("this master was given a -c")
            .as_str(),
        "/etc/nginx/glued.conf"
    );
    assert_eq!(
        master.prefix.expect("this master was given a -p").as_str(),
        "/srv/nginx"
    );
}

#[test]
fn the_workers_date_the_last_reload() {
    // Arrange: a reload replaces every worker and leaves the master alone, so the oldest
    // worker is when the configuration was last read. The master's own start time cannot
    // answer that.
    let proc = proc_tree("workers", MASTER);

    // Act
    let master = master_process::find_in(&proc, &proc.join("nginx"))
        .expect("the fixture is readable")
        .expect("the fixture holds a master");

    // Assert
    assert_eq!(master.worker_count, 2);
    assert_eq!(
        master
            .workers_started_at
            .expect("the fixture has workers")
            .as_i64(),
        started(&proc.join("5")).min(started(&proc.join("6")))
    );
}

#[test]
fn an_upgraded_binary_is_still_the_running_nginx() {
    // Arrange: after a package upgrade the kernel marks the link ` (deleted)`, and a
    // comparison that did not allow for it would report the server as not running at all.
    let proc = proc_tree("deleted", MASTER);
    fs::remove_file(proc.join("4/exe")).expect("a writable scratch tree");
    let replaced = format!("{} (deleted)", proc.join("nginx").display());
    symlink(&replaced, proc.join("4/exe")).expect("a writable scratch tree");

    // Act
    let master = master_process::find_in(&proc, &proc.join("nginx"))
        .expect("the fixture is readable")
        .expect("an upgraded nginx is still running");

    // Assert: the marker itself reaches the document, because it is the state.
    assert_eq!(
        master
            .executable
            .expect("the link is readable, it is its target that is gone")
            .as_str(),
        replaced
    );
}

#[test]
fn an_nginx_that_is_not_running_is_not_invented() {
    // Arrange: the binary is installed and no process is running it.
    let proc = scratch_tree("nginx-master-stopped", &[]);

    // Act
    let master =
        master_process::find_in(&proc, Path::new("/usr/sbin/nginx")).expect("an empty tree reads");

    // Assert
    assert_eq!(master, None);
}

#[test]
fn a_master_whose_executable_cannot_be_read_is_still_a_master() {
    // Arrange: only root, or the process's own owner, may read `/proc/<pid>/exe`. An
    // unprivileged run gets EACCES, and reporting that as "nginx is not running" would be a
    // confident lie about the host. The fixture stands in for it with no link at all.
    let proc = proc_tree("unreadable", MASTER);
    fs::remove_file(proc.join("4/exe")).expect("a writable scratch tree");

    // Act
    let master = master_process::find_in(&proc, &proc.join("nginx"))
        .expect("the fixture is readable")
        .expect("a process that calls itself an nginx master is one");

    // Assert
    assert_eq!(master.process_id, 4);
    assert_eq!(master.executable, None);
}
