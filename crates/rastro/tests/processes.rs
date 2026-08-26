//! Reading the process table, without needing a real `/proc` to read it from.
//!
//! The walk is exercised against a directory tree shaped like procfs, because the facts it
//! reads — is the entry numeric, does the file still exist, does `exe` resolve — are exactly
//! the ones a mock would have to invent.

use std::fs;
use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::processes::{
    ProcProcesses, Process, ProcessTable, ProcessesCollector, proc_cmdline, proc_status,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Observation, View};
use support::fs_tree::scratch_tree;
use support::observation::{items_of, keys_of};
/// `/proc/1/status` as the development box writes it, trimmed to the lines rastro reads.
const SYSTEMD_STATUS: &str = "\
Name:\tsystemd
Umask:\t0000
State:\tS (sleeping)
Tgid:\t1
Pid:\t1
PPid:\t0
Uid:\t0\t0\t0\t0
Gid:\t0\t0\t0\t0
Threads:\t1
";

/// A process that dropped privileges: real id 1000, effective id 0.
const DROPPED_STATUS: &str = "\
Name:\tnode_exporter
State:\tS (sleeping)
PPid:\t1
Uid:\t1000\t0\t0\t0
Gid:\t1000\t0\t0\t0
Threads:\t8
";

fn tree(name: &str) -> PathBuf {
    scratch_tree(&format!("processes-{name}"), &["self"])
}

/// Writes one process directory. `cmdline` is written verbatim, so a test can spell the NUL
/// separators itself.
fn write_process(root: &Path, pid: u32, status: &str, cmdline: &str, cgroup: Option<&str>) {
    let directory = root.join(pid.to_string());
    fs::create_dir_all(&directory).expect("a writable process directory");
    fs::write(directory.join("status"), status).expect("a writable status");
    fs::write(directory.join("cmdline"), cmdline).expect("a writable cmdline");
    if let Some(cgroup) = cgroup {
        fs::write(directory.join("cgroup"), cgroup).expect("a writable cgroup");
    }
}

fn read(root: &Path) -> ProcessTable {
    ProcProcesses::at(root)
        .read()
        .expect("this tree is well formed")
}

fn named(table: &ProcessTable, name: &str) -> Process {
    table
        .processes()
        .iter()
        .find(|process| process.name.as_str() == name)
        .unwrap_or_else(|| panic!("expected a process named {name}"))
        .clone()
}

#[test]
fn read_reads_a_process_from_its_status_file() {
    // Arrange
    let root = tree("one-process");
    write_process(
        &root,
        1,
        SYSTEMD_STATUS,
        "/lib/systemd/systemd\0--system\0",
        Some("0::/init.scope\n"),
    );

    // Act
    let systemd = named(&read(&root), "systemd");

    // Assert
    assert_eq!(systemd.process_id.as_u32(), 1);
    assert_eq!(systemd.parent_process_id.as_u32(), 0);
    assert_eq!(systemd.state.as_str(), "S (sleeping)");
    assert_eq!(systemd.thread_count, 1);
    assert_eq!(systemd.user_id, 0);
}

#[test]
fn read_takes_the_real_id_rather_than_the_effective_one() {
    // Arrange: the kernel writes real, effective, saved-set and filesystem ids in that
    // order, and a process that dropped privileges keeps its real id.
    let root = tree("dropped-privileges");
    write_process(&root, 42, DROPPED_STATUS, "/usr/bin/node_exporter\0", None);

    // Act
    let exporter = named(&read(&root), "node_exporter");

    // Assert: 1000, the account the process belongs to, not the 0 it is acting as.
    assert_eq!(exporter.user_id, 1000);
    assert_eq!(exporter.group_id, 1000);
}

#[test]
fn read_splits_a_command_line_on_its_nul_separators() {
    // Arrange: the kernel separates arguments with NUL precisely so an argument containing a
    // space is unambiguous.
    let root = tree("command-line");
    write_process(
        &root,
        7,
        SYSTEMD_STATUS,
        "/usr/bin/thing\0--filter=a b\0--port\x005432\0",
        None,
    );

    // Act
    let process = named(&read(&root), "systemd");

    // Assert: four arguments, and the one with a space stayed one argument.
    assert_eq!(
        process.command_line.arguments(),
        ["/usr/bin/thing", "--filter=a b", "--port", "5432"]
    );
}

#[test]
fn read_drops_exactly_one_trailing_nul() {
    // Act: a naive split yields a spurious empty argument at the end.
    let parsed = proc_cmdline::parse_arguments("/bin/sh\0-c\0true\0");

    // Assert
    assert_eq!(parsed.arguments(), ["/bin/sh", "-c", "true"]);
}

#[test]
fn read_keeps_a_genuine_empty_final_argument() {
    // Act: a program invoked with `""` really has one, so trimming all trailing empties
    // would delete it.
    let parsed = proc_cmdline::parse_arguments("/bin/echo\0\0");

    // Assert
    assert_eq!(parsed.arguments(), ["/bin/echo", ""]);
}

#[test]
fn read_reads_a_kernel_thread_as_having_no_command_line() {
    // Arrange: this is how a kernel thread is told apart from userspace.
    let root = tree("kernel-thread");
    write_process(
        &root,
        2,
        "Name:\tkthreadd\nState:\tS (sleeping)\nPPid:\t0\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\nThreads:\t1\n",
        "",
        None,
    );

    // Act
    let thread = named(&read(&root), "kthreadd");

    // Assert
    assert!(thread.command_line.is_empty());
    assert_eq!(thread.executable, None);
}

#[test]
fn read_reads_the_unified_cgroup_line() {
    // Arrange: on a systemd box the cgroup path names the unit a process belongs to, which
    // is the only link between this facet and `units`.
    let root = tree("cgroup");
    write_process(
        &root,
        99,
        SYSTEMD_STATUS,
        "/usr/sbin/sshd\0",
        Some("0::/system.slice/ssh.service\n"),
    );

    // Act
    let process = named(&read(&root), "systemd");

    // Assert
    assert_eq!(
        process.control_group.as_ref().map(|group| group.as_str()),
        Some("/system.slice/ssh.service")
    );
}

#[test]
fn read_reports_no_control_group_on_a_host_with_a_v1_hierarchy() {
    // Arrange: cgroup v1 writes one line per controller, with no single answer to give, so
    // picking one arbitrarily would invent a fact.
    let root = tree("cgroup-v1");
    write_process(
        &root,
        5,
        SYSTEMD_STATUS,
        "/x\0",
        Some("11:devices:/\n10:memory:/\n"),
    );

    // Act
    let process = named(&read(&root), "systemd");

    // Assert
    assert_eq!(process.control_group, None);
}

#[test]
fn read_ignores_everything_under_proc_that_is_not_a_process() {
    // Arrange: `/proc` is full of non-numeric entries, and a filter on "is it a directory"
    // would sweep in `net`, `sys` and `self`.
    let root = tree("non-numeric");
    write_process(&root, 1, SYSTEMD_STATUS, "/x\0", None);
    fs::create_dir_all(root.join("net")).expect("a writable directory");
    fs::create_dir_all(root.join("sys")).expect("a writable directory");
    fs::write(root.join("uptime"), "123 456\n").expect("a writable file");

    // Act
    let table = read(&root);

    // Assert
    assert_eq!(table.len(), 1);
}

#[test]
fn read_skips_a_process_that_left_before_rastro_reached_it() {
    // Arrange: a directory that exists with no `status` in it is what a process that exited
    // mid-walk looks like. Listing and then reading is inherently racy, and on a busy box
    // this happens often.
    let root = tree("departed");
    write_process(&root, 1, SYSTEMD_STATUS, "/x\0", None);
    fs::create_dir_all(root.join("4242")).expect("a writable directory");

    // Act
    let table = read(&root);

    // Assert: the process really did exit, so it is dropped rather than failing the run.
    assert_eq!(table.len(), 1);
}

#[test]
fn read_fails_when_a_status_file_is_there_and_is_not_what_rastro_expects() {
    // Arrange: the distinction that makes the skip above safe. A file that is gone means the
    // process left; a file that is there and will not parse means the interface is not what
    // rastro believes.
    let root = tree("malformed");
    write_process(&root, 1, "this is not a status file\n", "/x\0", None);

    // Act
    let result = ProcProcesses::at(&root).read();

    // Assert
    let failure = result.expect_err("a malformed status must be loud");
    assert!(
        failure.to_string().contains("Name"),
        "the message must name the missing field, got: {failure}"
    );
}

#[test]
fn read_sorts_the_table_rather_than_leaving_it_in_pid_order() {
    // Arrange: the kernel's order is by pid, so a daemon that restarts jumps to the end and
    // every entry after it shifts.
    let root = tree("sorted");
    write_process(
        &root,
        9,
        "Name:\tzulu\nState:\tS\nPPid:\t1\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\nThreads:\t1\n",
        "/z\0",
        None,
    );
    write_process(
        &root,
        3,
        "Name:\talpha\nState:\tS\nPPid:\t1\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\nThreads:\t1\n",
        "/a\0",
        None,
    );

    // Act
    let table = read(&root);

    // Assert
    assert_eq!(
        table
            .processes()
            .iter()
            .map(|process| process.name.as_str())
            .collect::<Vec<&str>>(),
        ["alpha", "zulu"]
    );
}

#[test]
fn presence_is_present_when_procfs_is_mounted() {
    // Arrange
    let root = tree("presence-present");

    // Act & Assert
    assert_eq!(
        ProcessesCollector::reading(ProcProcesses::at(&root)).presence(),
        Presence::Present
    );
}

#[test]
fn presence_is_undetermined_when_procfs_is_not_mounted() {
    // Arrange: a running kernel always has processes, so there is no `absent` to give here
    // the way a kernel without module support gives one to the modules collector.
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("processes-unmounted");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a writable scratch directory");

    // Act & Assert
    match ProcessesCollector::reading(ProcProcesses::at(&root)).presence() {
        Presence::Undetermined { reason } => assert!(
            reason.contains("not mounted"),
            "the reason must say what was missing, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn status_reads_a_field_with_tabs_in_its_value() {
    // Act: the `Uid:` line is four tab-separated ids.
    let fields = proc_status::parse(SYSTEMD_STATUS);

    // Assert
    assert_eq!(
        proc_status::field(&fields, proc_status::USER).expect("a Uid line"),
        "0\t0\t0\t0"
    );
}

#[test]
fn status_fails_loudly_for_a_field_the_kernel_did_not_write() {
    // Act & Assert
    let fields = proc_status::parse("Name:\tx\n");
    assert!(proc_status::field(&fields, proc_status::THREADS).is_err());
}

#[test]
fn no_process_reaches_the_diffable_view() {
    // Arrange: the facet's central property. A process table cannot be byte-identical on a
    // machine that is doing anything, and byte-identity is the contract every other facet
    // rests on, so the whole table is volatile. Two narrower rules preceded this one and the
    // collector's documentation records why each was not enough.
    let root = tree("nothing-diffable");
    write_process(
        &root,
        1,
        SYSTEMD_STATUS,
        "/lib/systemd/systemd\0--system\0",
        Some("0::/init.scope\n"),
    );
    write_process(&root, 42, DROPPED_STATUS, "/usr/bin/node_exporter\0", None);

    // Act
    let observation = Observation::from(&read(&root));
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet itself survives, empty");

    // Assert
    assert_eq!(
        items_of(&observation).len(),
        2,
        "both are in the complete view"
    );
    assert!(
        items_of(&diffable).is_empty(),
        "no process may reach the diffable view, got {:?}",
        items_of(&diffable)
    );
}

#[test]
fn the_complete_view_keeps_every_field() {
    // Act: `--include-volatile` is where this facet lives, so nothing may be missing there.
    let root = tree("complete");
    write_process(
        &root,
        1,
        SYSTEMD_STATUS,
        "/lib/systemd/systemd\0",
        Some("0::/init.scope\n"),
    );

    // Assert
    assert_eq!(
        keys_of(&items_of(&Observation::from(&read(&root)))[0]),
        [
            "command_line",
            "control_group",
            "executable",
            "group_id",
            "name",
            "parent_process_id",
            "process_id",
            "state",
            "thread_count",
            "user_id"
        ]
    );
}

#[test]
fn the_collector_reports_its_second_version() {
    // Act: the data is unchanged and its visibility is not, so a consumer that saw processes
    // in a default run and now sees none must be able to tell the collector moved.
    let collector = ProcessesCollector::reading(ProcProcesses::new());

    // Assert
    assert_eq!(collector.identity().version.as_str(), "2");
}
