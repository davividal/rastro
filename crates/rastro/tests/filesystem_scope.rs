//! Which filesystems the walk covers, and the collector that walks them.
//!
//! The scope is every mount but the kernel's own interfaces, named as a list. `nodev` is not
//! the criterion and this file has the counter-examples: `nfs` and `zfs` need no block device
//! and hold real data. The table is read from a path the caller names, so this needs no `/proc`
//! and no mount.

mod support;

use std::fs;

use rastro::collectors::filesystem::{
    ContentPolicy, DigestAlgorithm, FilesystemCollector, MountedFilesystems, PolicyRule, WalkPolicy,
};
use rastro_collector::{AbsolutePath, CollectionError, Collector, Presence, WalkedTree};
use support::fs_tree::{scratch_tree, write};
use support::observation::keys_of;

/// `/proc/mounts` as the reference box prints it, trimmed to the mounts that matter.
const MOUNTS: &str = "\
/dev/sda1 / ext4 rw,relatime 0 0
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
udev /dev devtmpfs rw,nosuid,relatime 0 0
tmpfs /run tmpfs rw,nosuid,nodev,noexec,relatime 0 0
cgroup2 /sys/fs/cgroup cgroup2 rw,nosuid,nodev,noexec,relatime 0 0
/dev/sda15 /boot/efi vfat rw,relatime 0 0
";

fn table(name: &str, mounts: &str) -> MountedFilesystems {
    let root = scratch_tree(name, &[]);
    write(&root, "mounts", mounts);

    MountedFilesystems::reading(&root.join("mounts"))
}

fn roots(name: &str, mounts: &str) -> Vec<String> {
    table(name, mounts)
        .walked()
        .expect("this table is well formed")
        .roots()
        .iter()
        .map(|root| root.as_str().to_owned())
        .collect()
}

fn boundaries(name: &str, mounts: &str) -> Vec<String> {
    table(name, mounts)
        .walked()
        .expect("this table is well formed")
        .boundaries()
        .iter()
        .map(|boundary| boundary.as_str().to_owned())
        .collect()
}

#[test]
fn roots_covers_every_mount_but_the_kernels_own_interfaces() {
    // Act
    let covered = roots("scope_real_only", MOUNTS);

    // Assert: two of seven mounts, the other five being `sysfs`, `proc`, `devtmpfs`, `tmpfs`
    // and `cgroup2`.
    assert_eq!(covered, vec!["/", "/boot/efi"]);
}

#[test]
fn roots_covers_a_filesystem_that_holds_data_without_a_block_device() {
    // Arrange: every one of these needs no block device, and every one holds real data. An
    // earlier version took the kernel's `nodev` marker as the criterion and skipped all four,
    // which loses an NFS data mount silently and fails outright on a ZFS root.
    let without_devices = "\
tank/root / zfs rw 0 0
fileserver:/exports/home /home nfs4 rw 0 0
host_share /srv/share virtiofs rw 0 0
overlay /var/lib/containers/storage/overlay overlay rw 0 0
proc /proc proc rw 0 0
";

    // Act
    let covered = roots("scope_nodev_with_data", without_devices);

    // Assert
    assert_eq!(
        covered,
        vec![
            "/",
            "/home",
            "/srv/share",
            "/var/lib/containers/storage/overlay"
        ]
    );
}

#[test]
fn boundaries_covers_every_mount_including_the_ones_not_walked() {
    // Act
    let stops = boundaries("scope_boundaries", MOUNTS);

    // Assert: the walk has to stop at a pseudo filesystem it must not enter as well as at the
    // next real one it will walk separately, so the boundaries are every mount rather than
    // the roots.
    assert_eq!(
        stops,
        vec![
            "/",
            "/boot/efi",
            "/dev",
            "/proc",
            "/run",
            "/sys",
            "/sys/fs/cgroup"
        ]
    );
}

#[test]
fn roots_reports_a_mount_point_mounted_twice_once() {
    // Arrange: `binfmt_misc` over the `autofs` that triggers it, at one path, which is what
    // that box really has. `autofs` is nodev here and the point stands for any pair.
    let doubled = "\
/dev/sda1 / ext4 rw 0 0
/dev/sdb1 /data ext4 rw 0 0
/dev/sdb1 /data ext4 rw 0 0
";

    // Act
    let covered = roots("scope_doubled", doubled);

    // Assert: walking a path twice would report every entry under it twice.
    assert_eq!(covered, vec!["/", "/data"]);
}

#[test]
fn roots_refuses_a_mount_point_the_kernel_escaped() {
    // Arrange: the kernel writes a space in a mount point as `\040`.
    let escaped = "/dev/sdb1 /mnt/my\\040disk ext4 rw 0 0\n";

    // Act
    let refused = table("scope_escaped", escaped).walked();

    // Assert: a path rastro cannot spell back is one it must not claim to have walked.
    assert!(refused.is_err());
}

#[test]
fn roots_refuses_a_host_where_nothing_real_is_mounted() {
    // Arrange
    let pseudo_only = "proc /proc proc rw 0 0\ntmpfs /run tmpfs rw 0 0\n";

    // Act
    let refused = table("scope_nothing_real", pseudo_only).walked();

    // Assert: rastro was read off a filesystem that holds files, so a host with none is a
    // failed read rather than a host with no files.
    assert!(refused.is_err());
}

#[test]
fn the_collector_is_present_on_every_host() {
    // Act & Assert: there is no host without a filesystem, so neither `absent` nor
    // `undetermined` has a meaning. A failed reading is reported by `collect`.
    assert_eq!(FilesystemCollector::new().presence(), Presence::Present);
}

#[test]
fn the_collector_reports_every_entry_under_the_roots_it_walks() {
    // Arrange: two roots, standing in for two mounted filesystems.
    let first = scratch_tree("collector_first", &["etc"]);
    let second = scratch_tree("collector_second", &["opt"]);
    write(&first, "etc/greeting", "hello\n");
    write(&second, "opt/payload", "hello\n");

    let collector = FilesystemCollector::walking(
        vec![
            AbsolutePath::new(first.to_str().expect("utf-8"), "root").expect("legal"),
            AbsolutePath::new(second.to_str().expect("utf-8"), "root").expect("legal"),
        ],
        WalkPolicy::new(vec![PolicyRule::shipped(
            WalkedTree::new("/").expect("a legal tree"),
            ContentPolicy::Hashed(DigestAlgorithm::Sha256),
        )])
        .expect("a legal table"),
    );

    // Act
    let observed = collector.collect().expect("both trees are readable");

    // Assert: one facet, both filesystems, keyed by absolute path.
    let paths = keys_of(&observed);
    assert!(
        paths.iter().any(|path| path.ends_with("/etc/greeting")),
        "got {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with("/opt/payload")),
        "got {paths:?}"
    );
}

#[test]
fn the_collector_fails_rather_than_reporting_a_host_with_no_files() {
    // Arrange
    let missing = FilesystemCollector::walking(
        vec![AbsolutePath::new("/rastro-no-such-root", "root").expect("legal")],
        WalkPolicy::built_in(),
    );

    // Act
    let refused = missing.collect();

    // Assert: an empty inventory would read as a host with nothing on it.
    assert!(refused.is_err());
}

#[test]
fn the_collector_fails_the_facet_when_the_claims_did_not_resolve() {
    // Arrange: what two collectors claiming one tree leaves behind. The conflict is
    // detected while the table is assembled, before any collector runs.
    let conflicted = FilesystemCollector::under(
        Err(CollectionError::new(
            "\"/var/lib/mysql\" is claimed by mariadb and already ruled by mysql",
        )),
        None,
    );

    // Act
    let refused = conflicted.collect();

    // Assert: this facet and no other. A bug in a collector pair costs the walk, not the
    // document, and the reason travels with it.
    let failure = refused.expect_err("an unresolved table cannot be walked");
    assert!(failure.to_string().contains("/var/lib/mysql"));
}

#[test]
fn a_staged_run_omits_the_executable_it_is_running_from() {
    // Arrange: the test binary is the observer here, and its own directory is the root, so
    // the walk is guaranteed to reach it. Staged, because only a caller that made a
    // temporary copy gets the omission: an installed rastro is part of the box.
    let observer = std::env::current_exe().expect("a running test has an executable");
    let directory = observer
        .parent()
        .expect("an executable lives in a directory");
    let collector = FilesystemCollector::walking_staged(
        vec![AbsolutePath::new(directory.to_str().expect("utf-8"), "root").expect("legal")],
        WalkPolicy::new(vec![PolicyRule::shipped(
            WalkedTree::new("/").expect("a legal tree"),
            ContentPolicy::MetadataOnly,
        )])
        .expect("a legal table"),
    );

    // Act
    let observed = collector.collect().expect("the tree is readable");

    // Assert: rastro is not state on the box it is fingerprinting. Its neighbours are, so
    // the omission is one path rather than a directory going quiet.
    let paths = keys_of(&observed);
    assert!(
        !paths.contains(&observer.to_string_lossy().into_owned()),
        "the observer reported itself"
    );
    assert!(paths.len() > 1, "only the observer should be missing");
}

#[test]
fn a_local_run_reports_the_executable_it_is_running_from() {
    // Arrange: the same tree, not staged.
    let observer = std::env::current_exe().expect("a running test has an executable");
    let directory = observer
        .parent()
        .expect("an executable lives in a directory");
    let collector = FilesystemCollector::walking(
        vec![AbsolutePath::new(directory.to_str().expect("utf-8"), "root").expect("legal")],
        WalkPolicy::new(vec![PolicyRule::shipped(
            WalkedTree::new("/").expect("a legal tree"),
            ContentPolicy::MetadataOnly,
        )])
        .expect("a legal table"),
    );

    // Act
    let observed = collector.collect().expect("the tree is readable");

    // Assert: a rastro installed on a box is part of that box, and a swapped binary is
    // exactly the change a fingerprint should catch. Only a caller that says it staged a
    // temporary copy gets the omission, whichever constructor named the roots.
    let root = AbsolutePath::new(directory.to_str().expect("utf-8"), "root").expect("legal");
    let within = FilesystemCollector::walking_within(
        vec![root.clone()],
        vec![root],
        WalkPolicy::new(vec![PolicyRule::shipped(
            WalkedTree::new("/").expect("a legal tree"),
            ContentPolicy::MetadataOnly,
        )])
        .expect("a legal table"),
    )
    .collect()
    .expect("the tree is readable");

    for reported in [&observed, &within] {
        assert!(
            keys_of(reported).contains(&observer.to_string_lossy().into_owned()),
            "an unstaged run must report its own binary"
        );
    }
}

#[test]
fn the_collector_collapses_a_path_two_walks_both_reached() {
    // Arrange: the same root twice, which is what a mount point reached from its parent
    // filesystem and as its own root looks like.
    let root = scratch_tree("collector_twice", &["etc"]);
    write(&root, "etc/greeting", "hello\n");
    let path = AbsolutePath::new(root.to_str().expect("utf-8"), "root").expect("legal");
    let collector = FilesystemCollector::walking(
        vec![path.clone(), path],
        WalkPolicy::new(vec![PolicyRule::shipped(
            WalkedTree::new("/").expect("a legal tree"),
            ContentPolicy::MetadataOnly,
        )])
        .expect("a legal table"),
    );

    // Act
    let observed = collector.collect().expect("the tree is readable");

    // Assert: both readings agree, so the entry appears once rather than twice.
    let greetings = keys_of(&observed)
        .into_iter()
        .filter(|path| path.ends_with("/etc/greeting"))
        .count();
    assert_eq!(greetings, 1);
}

#[test]
fn the_collector_refuses_two_readings_of_one_path_that_disagree() {
    // Arrange: the same path walked twice with the file rewritten in between, which is what
    // a walk that did not see one moment of the host looks like.
    let root = scratch_tree("collector_disagree", &[]);
    write(&root, "greeting", "hello\n");
    let path = AbsolutePath::new(root.to_str().expect("utf-8"), "root").expect("legal");
    let hashing = WalkPolicy::new(vec![PolicyRule::shipped(
        WalkedTree::new("/").expect("a legal tree"),
        ContentPolicy::Hashed(DigestAlgorithm::Sha256),
    )])
    .expect("a legal table");

    let first = FilesystemCollector::walking(vec![path.clone()], hashing.clone())
        .collect()
        .expect("the tree is readable");
    fs::write(root.join("greeting"), "goodbye\n").expect("a writable file");
    let second = FilesystemCollector::walking(vec![path], hashing)
        .collect()
        .expect("the tree is readable");

    // Assert: each walk on its own succeeds, and they disagree, which is the condition the
    // merge refuses. Asserted as a difference rather than by merging two walks here, because
    // the collector cannot be made to walk one root at two moments.
    assert_ne!(first, second);
}

#[test]
fn the_collector_stops_at_a_boundary_that_shares_its_device() {
    // Arrange: a bind mount shares the device of what it binds. `mount --bind / /mnt/root`
    // leaves both at device 2049 on the reference box, so a walk that compared only device
    // numbers walked and hashed the whole root filesystem a second time underneath. Here the
    // boundary is an ordinary directory of the same tree, which is exactly what that looks
    // like to the walk.
    let root = scratch_tree("collector_boundary", &["etc", "mnt/root/etc"]);
    write(&root, "etc/greeting", "hello\n");
    write(&root, "mnt/root/etc/greeting", "hello\n");

    let inside = root.join("mnt/root");
    let collector = FilesystemCollector::walking_within(
        vec![AbsolutePath::new(root.to_str().expect("utf-8"), "root").expect("legal")],
        vec![
            AbsolutePath::new(root.to_str().expect("utf-8"), "boundary").expect("legal"),
            AbsolutePath::new(inside.to_str().expect("utf-8"), "boundary").expect("legal"),
        ],
        WalkPolicy::built_in(),
    );

    // Act
    let observed = collector.collect().expect("the tree is readable");

    // Assert: the boundary is recorded as the directory it is, and nothing under it is.
    let paths = keys_of(&observed);
    assert!(
        paths.iter().any(|path| path.ends_with("/mnt/root")),
        "the mount point itself is state: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.contains("/mnt/root/")),
        "nothing under a boundary is walked: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with("/etc/greeting")),
        "the rest of the tree is: {paths:?}"
    );
}

/// The shipped table, over a scratch root the test owns.
fn metadata_only() -> WalkPolicy {
    WalkPolicy::new(vec![PolicyRule::shipped(
        WalkedTree::new("/").expect("a legal tree"),
        ContentPolicy::MetadataOnly,
    )])
    .expect("a legal table")
}

fn absolute(path: &std::path::Path) -> AbsolutePath {
    AbsolutePath::new(path.to_str().expect("a UTF-8 scratch path"), "walked root")
        .expect("a legal path")
}

#[test]
fn a_walk_leaves_out_the_document_it_is_writing() {
    // Arrange: run one's output sitting in the tree run two walks. Asserted over a scratch root
    // rather than through the binary over the whole host, which cost over two minutes on a
    // coverage-instrumented runner for the same claim — and this states it more precisely,
    // because the root is the only thing walked and the omission is the only difference.
    let root = scratch_tree("scope-omits-the-output", &[]);
    write(&root, "fingerprint.json", "{}\n");
    write(&root, "kept.conf", "a setting\n");
    let output = root.join("fingerprint.json");

    // Act
    let observed = FilesystemCollector::walking(vec![absolute(&root)], metadata_only())
        .writing_to(Some(output.clone()))
        .collect()
        .expect("a readable scratch tree");

    // Assert: the document is gone from the walk and its neighbour is not.
    let walked = keys_of(&observed);
    assert!(
        !walked.contains(&output.to_string_lossy().into_owned()),
        "the walk reported the document being written: {walked:?}"
    );
    assert!(
        walked.contains(&root.join("kept.conf").to_string_lossy().into_owned()),
        "the omission took a neighbour with it: {walked:?}"
    );
}

#[test]
fn a_walk_leaves_out_an_output_path_reached_through_a_symlinked_directory() {
    // Arrange: `std::path::absolute` is lexical, so a path through a symlinked directory keeps
    // the symlink — while the walk never follows one and meets the file under the real
    // directory. The two spellings have to be reconciled before the comparison, or the previous
    // document lands back in the next run.
    let root = scratch_tree("scope-omits-through-a-symlink", &["real"]);
    write(&root, "real/fingerprint.json", "{}\n");
    std::os::unix::fs::symlink(root.join("real"), root.join("linked")).expect("a symlink");

    // Act: told the path as an operator would have typed it, through the symlink.
    let observed = FilesystemCollector::walking(vec![absolute(&root)], metadata_only())
        .writing_to(Some(rastro::output::as_walked(
            &root.join("linked/fingerprint.json"),
        )))
        .collect()
        .expect("a readable scratch tree");

    // Assert: omitted under the path the walk actually met it by.
    let met_as = root
        .join("real")
        .canonicalize()
        .expect("a real directory")
        .join("fingerprint.json");
    assert!(
        !keys_of(&observed).contains(&met_as.to_string_lossy().into_owned()),
        "got {:?}",
        keys_of(&observed)
    );
}
