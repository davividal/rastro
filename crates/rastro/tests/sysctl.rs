//! Reading the kernel's runtime parameters, without needing a `/proc` to read
//! them from.
//!
//! The walk is exercised against a real directory tree rather than a mocked
//! filesystem, because the three facts it reads about an entry (is it a
//! directory, what is its mode, does reading it work) are exactly the ones a mock
//! would have to invent. `CARGO_TARGET_TMPDIR` is cargo's own per-target scratch
//! directory, so this needs no dependency.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::sysctl::{
    ProcSys, SysctlCollector, SysctlKey, SysctlParameters, SysctlValue, proc_sys_entry,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};
use support::fs_tree::scratch_tree;
use support::observation::{keys_of, object_of};

/// A mode with no read bit for anyone, which is how the kernel marks the entries
/// that are triggers rather than settings.
const WRITE_ONLY: u32 = 0o200;

/// The mode of an ordinary parameter.
const READABLE: u32 = 0o644;

fn key(name: &str) -> SysctlKey {
    let segments: Vec<String> = name.split('.').map(str::to_owned).collect();
    SysctlKey::of(&segments).expect("a legal sysctl key")
}

fn segments(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// A fresh, empty tree named after the test that asked for it.
///
/// Named rather than random so that a failing test's tree can be inspected, and
/// removed first rather than last so that it survives the failure for that.
fn tree(name: &str) -> PathBuf {
    scratch_tree(name, &[])
}

fn write(root: &Path, relative: &str, contents: &str, mode: u32) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("a writable tree");
    fs::write(&path, contents).expect("a writable parameter");
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("a settable mode");
}

/// The walk over a tree, with `/proc` pointed at the tree's own parent so that
/// presence never depends on the machine running the test.
fn read(root: &Path) -> SysctlParameters {
    ProcSys::at(root, root)
        .read()
        .expect("this tree is well formed")
}

fn value_of(parameters: &SysctlParameters, name: &str) -> SysctlValue {
    parameters
        .get(name)
        .unwrap_or_else(|| panic!("expected a {name:?} parameter, got {parameters:?}"))
        .clone()
}

fn text_of(parameters: &SysctlParameters, name: &str) -> String {
    value_of(parameters, name)
        .as_str()
        .unwrap_or_else(|| panic!("expected {name:?} to have been reported"))
        .to_owned()
}

#[test]
fn of_joins_the_segments_with_dots() {
    // Act
    let key = SysctlKey::of(&segments(&["net", "ipv4", "ip_forward"])).expect("a legal key");

    // Assert
    assert_eq!(key.as_str(), "net.ipv4.ip_forward");
}

#[test]
fn of_swaps_a_dot_inside_a_segment_for_a_slash() {
    // Arrange: this box really publishes `fs/binfmt_misc/python3.11`, so the
    // segment's own dot has to be told apart from the ones that separate segments.
    let path = segments(&["fs", "binfmt_misc", "python3.11"]);

    // Act
    let key = SysctlKey::of(&path).expect("a legal key");

    // Assert: joining naively would give `fs.binfmt_misc.python3.11`, which no
    // reader could split back into the path it came from.
    assert_eq!(key.as_str(), "fs.binfmt_misc.python3/11");
}

#[test]
fn of_swaps_every_dot_in_a_segment_not_only_the_first() {
    // Act
    let key = SysctlKey::of(&segments(&["net", "ipv4", "conf", "eth0.100.7"])).expect("legal");

    // Assert
    assert_eq!(key.as_str(), "net.ipv4.conf.eth0/100/7");
}

#[test]
fn of_refuses_an_empty_segment() {
    // Act: a nameless directory means the caller split a path with a double
    // separator, which is a misread rather than a parameter.
    let result = SysctlKey::of(&segments(&["net", "", "ip_forward"]));

    // Assert
    assert!(result.is_err());
}

#[test]
fn of_refuses_no_segments_at_all() {
    // Act & Assert
    assert!(SysctlKey::of::<String>(&[]).is_err());
}

#[test]
fn reported_drops_the_newline_the_kernel_ends_a_value_with() {
    // Act
    let value = SysctlValue::reported("debian12\n");

    // Assert: the newline is punctuation of the interface, not part of the value.
    assert_eq!(value, SysctlValue::Reported("debian12".to_owned()));
}

#[test]
fn reported_drops_only_the_last_newline() {
    // Arrange: `fs.binfmt_misc.*` genuinely spans several lines, one per field of
    // a registered format.
    let registration = "enabled\ninterpreter /usr/bin/qemu-arm\nflags: OCF\n";

    // Act
    let value = SysctlValue::reported(registration);

    // Assert: trimming them all would fuse three facts into one token.
    assert_eq!(
        value,
        SysctlValue::Reported("enabled\ninterpreter /usr/bin/qemu-arm\nflags: OCF".to_owned())
    );
}

#[test]
fn reported_keeps_a_value_that_is_only_a_newline_as_empty_text() {
    // Act: `net.ipv4.ip_local_reserved_ports` reads back as exactly this on a box
    // that has reserved no ports.
    let value = SysctlValue::reported("\n");

    // Assert: empty, and emphatically not withheld.
    assert_eq!(value, SysctlValue::Reported(String::new()));
}

#[test]
fn reported_keeps_the_tabs_a_value_is_separated_by() {
    // Act: `kernel.printk` is four numbers separated by tabs.
    let value = SysctlValue::reported("7\t4\t1\t7\n");

    // Assert
    assert_eq!(value.as_str(), Some("7\t4\t1\t7"));
}

#[test]
fn classify_skips_an_entry_nobody_is_allowed_to_read() {
    // Act: `vm.drop_caches` is a button, not a setting.
    let classified = proc_sys_entry::classify(&segments(&["vm", "drop_caches"]), WRITE_ONLY, None)
        .expect("a write-only entry is not a failure");

    // Assert: it holds no state, so it belongs nowhere in the facet.
    assert_eq!(classified, None);
}

#[test]
fn classify_records_a_readable_entry_the_kernel_declined_as_withheld() {
    // Arrange: an unset `stable_secret` is advertised readable at 0600 and then
    // answers EIO, which reaches the walk as a failed read.
    let name = segments(&["net", "ipv6", "conf", "lo", "stable_secret"]);

    // Act
    let (key, value) = proc_sys_entry::classify(&name, 0o600, None)
        .expect("a declined read is not a collection failure")
        .expect("the parameter is readable, so it holds state");

    // Assert: visible as a parameter that has never been set, not as an absence.
    assert_eq!(key.as_str(), "net.ipv6.conf.lo.stable_secret");
    assert_eq!(value, SysctlValue::Withheld);
}

#[test]
fn classify_refuses_a_value_that_is_not_valid_utf8() {
    // Arrange: `kernel.core_pattern` holds whatever an operator wrote into it.
    let invalid = [0xff, 0xfe];

    // Act
    let result = proc_sys_entry::classify(
        &segments(&["kernel", "core_pattern"]),
        READABLE,
        Some(&invalid),
    );

    // Assert: refused rather than repaired with U+FFFD, and the message names the
    // parameter so an operator has something to grep for.
    let failure = result.expect_err("invalid UTF-8 must not be silently replaced");
    assert!(
        failure.to_string().contains("kernel.core_pattern"),
        "the message must name the parameter, got: {failure}"
    );
}

#[test]
fn read_names_a_parameter_after_the_path_it_was_published_under() {
    // Arrange
    let root = tree("read_names_a_parameter");
    write(&root, "net/ipv4/ip_forward", "1\n", READABLE);

    // Act
    let parameters = read(&root);

    // Assert
    assert_eq!(text_of(&parameters, "net.ipv4.ip_forward"), "1");
}

#[test]
fn read_walks_the_whole_tree_however_deep_it_goes() {
    // Arrange: five levels is ordinary, `net.ipv4.conf.<interface>.<parameter>`.
    let root = tree("read_walks_the_whole_tree");
    write(&root, "kernel/hostname", "debian12\n", READABLE);
    write(&root, "net/ipv4/conf/enp0s8/forwarding", "0\n", READABLE);
    write(&root, "vm/swappiness", "60\n", READABLE);

    // Act
    let parameters = read(&root);

    // Assert
    assert_eq!(parameters.len(), 3);
    assert_eq!(text_of(&parameters, "kernel.hostname"), "debian12");
    assert_eq!(text_of(&parameters, "net.ipv4.conf.enp0s8.forwarding"), "0");
    assert_eq!(text_of(&parameters, "vm.swappiness"), "60");
}

#[test]
fn read_omits_the_write_only_triggers_from_the_facet() {
    // Arrange
    let root = tree("read_omits_the_write_only_triggers");
    write(&root, "vm/swappiness", "60\n", READABLE);
    write(&root, "vm/drop_caches", "", WRITE_ONLY);
    write(&root, "vm/compact_memory", "", WRITE_ONLY);

    // Act
    let parameters = read(&root);

    // Assert: a button is not state, and dropping it by permission rather than by
    // name stays right when a kernel adds a sixth trigger.
    assert_eq!(
        parameters
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<&str>>(),
        ["vm.swappiness"]
    );
}

#[test]
fn read_orders_parameters_by_name_whatever_order_the_tree_yields() {
    // Arrange: written in an order that is neither sorted nor reverse sorted.
    let root = tree("read_orders_parameters_by_name");
    write(&root, "vm/swappiness", "60\n", READABLE);
    write(&root, "kernel/hostname", "debian12\n", READABLE);
    write(&root, "net/ipv4/ip_forward", "1\n", READABLE);
    write(&root, "fs/file-max", "9223372036854775807\n", READABLE);

    // Act
    let observation = Observation::from(&read(&root));

    // Assert: sorted by name, so the walk order of a directory never reaches the
    // document.
    assert_eq!(
        keys_of(&observation),
        [
            "fs.file-max",
            "kernel.hostname",
            "net.ipv4.ip_forward",
            "vm.swappiness"
        ]
    );
}

#[test]
fn read_keeps_an_empty_value_apart_from_a_withheld_one() {
    // Arrange
    let root = tree("read_keeps_an_empty_value_apart");
    write(&root, "net/ipv4/ip_local_reserved_ports", "\n", READABLE);

    // Act
    let observation = Observation::from(&read(&root));

    // Assert: text, not null. A parameter set to nothing has been set.
    let entries = object_of(&observation);
    let (_, reserved) = entries
        .iter()
        .find(|(key, _)| key == "net.ipv4.ip_local_reserved_ports")
        .expect("the parameter was written");
    assert_eq!(
        reserved.content(),
        &Content::Scalar(Scalar::Text(String::new()))
    );
}

#[test]
fn read_ignores_an_entry_that_is_neither_a_directory_nor_a_file() {
    // Arrange: the kernel publishes only those two kinds, so anything else is not
    // a parameter. A symlink is the reachable case, and following one out of the
    // tree would let a mount elsewhere masquerade as kernel state.
    let root = tree("read_ignores_a_symlink");
    write(&root, "kernel/hostname", "debian12\n", READABLE);
    std::os::unix::fs::symlink(root.join("kernel/hostname"), root.join("kernel/alias"))
        .expect("a creatable symlink");

    // Act
    let parameters = read(&root);

    // Assert
    assert_eq!(
        parameters
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<&str>>(),
        ["kernel.hostname"]
    );
}

#[test]
fn a_self_changing_parameter_is_marked_volatile() {
    // Arrange: measured, not assumed. Two snapshots of an idle box twenty seconds
    // apart disagreed on exactly this parameter, among six.
    let root = tree("a_self_changing_parameter");
    write(
        &root,
        "fs/file-nr",
        "864\t0\t9223372036854775807\n",
        READABLE,
    );
    write(&root, "net/core/somaxconn", "4096\n", READABLE);

    // Act
    let observation = Observation::from(&read(&root));

    // Assert: the same shape of value, and only one of them is noise. Nothing in
    // `864` betrays that it is a live count of open descriptors.
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");
    assert_eq!(keys_of(&diffable), ["net.core.somaxconn"]);
}

#[test]
fn the_uuid_parameter_is_volatile_because_it_differs_between_two_reads() {
    // Arrange: the extreme case, and the one that would have broken the
    // determinism harness by itself. The kernel mints a fresh UUID on every read,
    // so two reads inside a single run already disagree.
    let root = tree("the_uuid_parameter_is_volatile");
    write(
        &root,
        "kernel/random/uuid",
        "bcfa3615-91c7-4310-a230-be9832ec3471\n",
        READABLE,
    );

    // Act
    let observation = Observation::from(&read(&root));

    // Assert
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");
    assert!(
        keys_of(&diffable).is_empty(),
        "a value that changes within one run must not reach the diffable view"
    );
}

#[test]
fn a_per_interface_secret_is_marked_sensitive_by_its_last_segment() {
    // Arrange: interface names are not known ahead of time, so no full name can
    // list `net.ipv6.conf.<interface>.stable_secret`.
    let root = tree("a_per_interface_secret");
    write(
        &root,
        "net/ipv6/conf/enp0s8/stable_secret",
        "fd00::1\n",
        0o600,
    );

    // Act
    let parameters = read(&root);
    let observation = Observation::from(&parameters);

    // Assert
    let entries = object_of(&observation);
    let (_, secret) = entries
        .iter()
        .find(|(name, _)| name == "net.ipv6.conf.enp0s8.stable_secret")
        .expect("the parameter was written");
    assert_eq!(
        secret.sensitivity(),
        rastro_fingerprint::Sensitivity::Sensitive
    );
}

#[test]
fn the_fast_open_key_is_marked_sensitive() {
    // Act
    let signing_key = key("net.ipv4.tcp_fastopen_key");

    // Assert: it signs TCP Fast Open cookies and reads back in full as root.
    assert!(signing_key.holds_a_secret());
}

#[test]
fn an_ordinary_parameter_is_neither_volatile_nor_sensitive() {
    // Act
    let ordinary = key("net.ipv4.ip_forward");

    // Assert
    assert!(!ordinary.changes_on_its_own());
    assert!(!ordinary.holds_a_secret());
}

#[test]
fn presence_is_present_when_the_tree_is_there() {
    // Arrange
    let root = tree("presence_is_present");
    write(&root, "kernel/hostname", "debian12\n", READABLE);

    // Act & Assert
    let collector = SysctlCollector::reading(ProcSys::at(&root, &root));
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn presence_is_absent_when_procfs_is_mounted_without_a_sysctl_tree() {
    // Arrange: a kernel built without `CONFIG_SYSCTL` genuinely has no tunable
    // parameters, which is state rather than a failure.
    let procfs = tree("presence_is_absent");
    fs::create_dir_all(procfs.join("self")).expect("a writable tree");

    // Act & Assert
    let collector = SysctlCollector::reading(ProcSys::at(procfs.join("sys"), &procfs));
    assert_eq!(collector.presence(), Presence::Absent);
}

#[test]
fn presence_is_undetermined_when_procfs_is_not_mounted_at_all() {
    // Arrange: rastro cannot see kernel state here, and reporting "no parameters"
    // would be a confident lie.
    let procfs = tree("presence_is_undetermined");

    // Act
    let collector = SysctlCollector::reading(ProcSys::at(procfs.join("sys"), &procfs));

    // Assert
    match collector.presence() {
        Presence::Undetermined { reason } => assert!(
            reason.contains("not mounted"),
            "the reason must say what was missing, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn collect_reports_every_parameter_in_the_tree() {
    // Arrange
    let root = tree("collect_reports_every_parameter");
    write(&root, "kernel/hostname", "debian12\n", READABLE);
    write(&root, "vm/swappiness", "60\n", READABLE);

    // Act
    let collected = SysctlCollector::reading(ProcSys::at(&root, &root))
        .collect()
        .expect("this tree is well formed");

    // Assert
    assert_eq!(keys_of(&collected), ["kernel.hostname", "vm.swappiness"]);
}

#[test]
fn collect_fails_loudly_when_the_tree_cannot_be_listed() {
    // Arrange: a root that is not there at all, which `presence` would have caught
    // first. Reaching `collect` anyway must produce a failure, never an empty set
    // of parameters passed off as the truth.
    let root = tree("collect_fails_loudly").join("absent");

    // Act
    let result = SysctlCollector::reading(ProcSys::at(&root, &root)).collect();

    // Assert
    assert!(result.is_err());
}

#[test]
fn a_counter_the_kernel_maintains_is_volatile_even_where_it_reads_zero() {
    // Arrange: these were added after CI caught a `sysctl` divergence the development box
    // could not reproduce. It has no conntrack module, so `nf_conntrack_count` does not
    // exist there at all, while a runner with a container engine has one that moves with
    // every connection. The quota counters and `fs.aio-nr` are the same shape: idle here,
    // moving on a box doing the work they count.
    let root = tree("counters");
    write(&root, "fs/aio-nr", "0\n", READABLE);
    write(&root, "fs/quota/syncs", "358\n", READABLE);
    write(&root, "net/netfilter/nf_conntrack_count", "27\n", READABLE);
    write(&root, "net/core/somaxconn", "4096\n", READABLE);

    // Act
    let diffable = Observation::from(&read(&root))
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert: only the setting is left. A count of quota cache hits has no signal to lose.
    assert_eq!(keys_of(&diffable), ["net.core.somaxconn"]);
}

#[test]
fn a_read_only_parameter_is_not_treated_as_a_counter() {
    // Arrange: mode `0444` looks like a promising structural signal for "the kernel tells
    // you and you cannot set it", and it marks `kernel.osrelease` too. Dropping that from a
    // diff would be a disaster, which is why the volatile set is a name list.
    let root = tree("read-only-is-not-volatile");
    write(&root, "kernel/osrelease", "6.1.0-47-arm64\n", 0o444);
    write(&root, "kernel/ostype", "Linux\n", 0o444);

    // Act
    let diffable = Observation::from(&read(&root))
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    assert_eq!(keys_of(&diffable), ["kernel.osrelease", "kernel.ostype"]);
}
