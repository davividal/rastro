//! Walking a real directory tree.
//!
//! Exercised against a real tree rather than a mocked filesystem, for the reason the
//! sysctl walk gives: the facts this reads about an entry (its kind, its mode, whether
//! reading it works) are exactly the ones a mock would have to invent.
//! `CARGO_TARGET_TMPDIR` is cargo's own per-target scratch directory, so this needs no
//! dependency.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

use rastro::collectors::filesystem::{
    ContentPolicy, DigestAlgorithm, FileKind, FileTree, PolicyRule, WalkPolicy, WalkedTree,
};
use rastro_fingerprint::Observation;
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, integer, is_null, keys_of, text};

/// `sha256sum` of the four bytes below, which is the value the walk has to reproduce.
const HELLO_DIGEST: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
const HELLO: &str = "hello\n";

fn hashing_everything() -> WalkPolicy {
    WalkPolicy::new(vec![PolicyRule {
        tree: WalkedTree::new("/").expect("a legal tree"),
        content: ContentPolicy::Hashed(DigestAlgorithm::Sha256),
    }])
    .expect("a legal table")
}

/// Hashing everywhere except one tree, named absolutely because a policy names real
/// paths and the scratch root is where this walk really happens.
fn metadata_only_under(root: &Path, relative: &str) -> WalkPolicy {
    WalkPolicy::new(vec![
        PolicyRule {
            tree: WalkedTree::new("/").expect("a legal tree"),
            content: ContentPolicy::Hashed(DigestAlgorithm::Sha256),
        },
        PolicyRule {
            tree: WalkedTree::new(root.join(relative).to_str().expect("a UTF-8 path"))
                .expect("a legal tree"),
            content: ContentPolicy::MetadataOnly,
        },
    ])
    .expect("a legal table")
}

fn walked(
    root: &Path,
    policy: &WalkPolicy,
) -> Vec<(String, rastro::collectors::filesystem::FileEntry)> {
    FileTree::at(root)
        .walk(policy)
        .expect("a readable tree")
        .entries()
        .iter()
        .map(|entry| (relative(root, entry.path.as_str()), entry.clone()))
        .collect()
}

/// Paths relative to the scratch root, so an assertion does not carry the temporary
/// directory's name.
fn relative(root: &Path, path: &str) -> String {
    Path::new(path)
        .strip_prefix(root)
        .map(|rest| rest.to_string_lossy().into_owned())
        .unwrap_or_else(|_| String::new())
}

fn entry_at<'a>(
    entries: &'a [(String, rastro::collectors::filesystem::FileEntry)],
    relative: &str,
) -> &'a rastro::collectors::filesystem::FileEntry {
    entries
        .iter()
        .find(|(name, _)| name == relative)
        .map(|(_, entry)| entry)
        .unwrap_or_else(|| {
            panic!(
                "expected an entry for {relative:?}, got {:?}",
                names(entries)
            )
        })
}

fn names(entries: &[(String, rastro::collectors::filesystem::FileEntry)]) -> Vec<&str> {
    entries.iter().map(|(name, _)| name.as_str()).collect()
}

fn tree_with_a_file(name: &str) -> PathBuf {
    let root = scratch_tree(name, &["etc"]);
    write(&root, "etc/greeting", HELLO);
    root
}

#[test]
fn walk_reports_the_root_every_directory_and_every_file() {
    // Arrange
    let root = scratch_tree("walk_reports_everything", &["etc/ssh", "opt"]);
    write(&root, "etc/ssh/sshd_config", HELLO);
    write(&root, "opt/payload", HELLO);

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: the root itself is state too, since its mode and owner are.
    assert_eq!(
        names(&entries),
        vec![
            "",
            "etc",
            "etc/ssh",
            "etc/ssh/sshd_config",
            "opt",
            "opt/payload"
        ]
    );
}

#[test]
fn walk_orders_entries_by_path() {
    // Arrange: created in an order that is not the sorted one, because a directory
    // read returns whatever order the filesystem feels like.
    let root = scratch_tree("walk_orders_entries", &[]);
    write(&root, "zulu", HELLO);
    write(&root, "alpha", HELLO);
    write(&root, "mike", HELLO);

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: list order is contractual, so the walk imposes it rather than inheriting
    // whatever order `read_dir` happened to give.
    assert_eq!(names(&entries), vec!["", "alpha", "mike", "zulu"]);
}

#[test]
fn walk_digests_a_regular_file_the_policy_hashes() {
    // Arrange
    let root = tree_with_a_file("walk_digests_a_file");

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert
    let digest = entry_at(&entries, "etc/greeting")
        .digest
        .as_ref()
        .expect("a hashed file carries its digest");
    assert_eq!(digest.algorithm(), DigestAlgorithm::Sha256);
    assert_eq!(digest.as_str(), HELLO_DIGEST);
}

#[test]
fn walk_leaves_a_metadata_only_tree_undigested() {
    // Arrange
    let root = tree_with_a_file("walk_leaves_metadata_only");

    // Act
    let entries = walked(&root, &metadata_only_under(&root, "etc"));

    // Assert: the entry is still recorded. Only its content goes unread, which is the
    // difference between a downgraded tree and an excluded one.
    let entry = entry_at(&entries, "etc/greeting");
    assert_eq!(entry.kind, FileKind::Regular);
    assert!(entry.digest.is_none());
    assert!(entry.size.is_some(), "size is metadata, not content");
}

#[test]
fn walk_records_a_directory_without_a_digest() {
    // Arrange
    let root = tree_with_a_file("walk_records_a_directory");

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: a directory has no content a digest could describe. What is in it is the
    // entries the walk reports separately.
    let directory = entry_at(&entries, "etc");
    assert_eq!(directory.kind, FileKind::Directory);
    assert!(directory.digest.is_none());
    assert!(directory.size.is_none());
}

#[test]
fn walk_records_a_symlink_target_without_following_it() {
    // Arrange
    let root = tree_with_a_file("walk_records_a_symlink");
    symlink("greeting", root.join("etc/enabled")).expect("a writable tree");

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: an enablement symlink under a `*.wants/` directory is exactly what this
    // tool exists to catch, so the link is the state, not what it points at. Following
    // it would report the target's content twice and miss the link entirely.
    let link = entry_at(&entries, "etc/enabled");
    assert_eq!(link.kind, FileKind::Symlink);
    assert_eq!(link.link_target.as_deref(), Some("greeting"));
    assert!(link.digest.is_none());

    // And the target is still reported once, as itself.
    assert_eq!(entry_at(&entries, "etc/greeting").kind, FileKind::Regular);
}

#[test]
fn walk_records_the_mode_the_owner_and_the_group() {
    // Arrange
    let root = tree_with_a_file("walk_records_the_mode");
    let path = root.join("etc/greeting");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("a writable file");
    let expected = fs::metadata(&path).expect("a readable file");

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: a permissions-only change is one of the two field-research cases this
    // walker exists for, so the mode is recorded exactly rather than as a summary.
    let entry = entry_at(&entries, "etc/greeting");
    assert_eq!(entry.mode.as_str(), "0640");
    assert_eq!(entry.owner, expected.uid() as i64);
    assert_eq!(entry.group, expected.gid() as i64);
}

#[test]
fn walk_records_the_inode_and_link_count_two_hardlinks_share() {
    // Arrange
    let root = tree_with_a_file("walk_records_a_hardlink");
    fs::hard_link(root.join("etc/greeting"), root.join("etc/also_greeting"))
        .expect("a writable tree");

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: two paths, one inode, and a link count that says so. Reporting the paths
    // and not the shared inode would read as two independent files.
    let first = entry_at(&entries, "etc/greeting");
    let second = entry_at(&entries, "etc/also_greeting");
    assert_eq!(first.inode, second.inode);
    assert_eq!(first.link_count, 2);
    assert_eq!(second.link_count, 2);
}

#[test]
fn walk_records_the_modification_and_status_change_times_the_kernel_holds() {
    // Arrange
    let root = tree_with_a_file("walk_records_the_times");
    let expected = fs::metadata(root.join("etc/greeting")).expect("a readable file");

    // Act
    let entries = walked(&root, &hashing_everything());

    // Assert: `st_mtim` is a second and a nanosecond, and the pair is carried whole.
    // Rounding to the second would make two writes inside one second the same fact.
    let entry = entry_at(&entries, "etc/greeting");
    assert_eq!(
        entry.modified.as_i64(),
        expected.mtime() * 1_000_000_000 + expected.mtime_nsec()
    );
    assert_eq!(
        entry.changed.as_i64(),
        expected.ctime() * 1_000_000_000 + expected.ctime_nsec()
    );
}

#[test]
fn walk_sees_a_rewrite_that_left_the_size_and_the_policy_unchanged() {
    // Arrange: a metadata-only tree, so no digest can be the thing that notices, and a
    // replacement of exactly the same length, so the size cannot be either.
    let root = tree_with_a_file("walk_sees_a_same_size_rewrite");
    let policy = metadata_only_under(&root, "etc");
    let before = entry_at(&walked(&root, &policy), "etc/greeting").clone();
    write(&root, "etc/greeting", "HELLO\n");

    // Act
    let after = entry_at(&walked(&root, &policy), "etc/greeting").clone();

    // Assert: this is the case the walk would otherwise report as no change at all.
    assert_eq!(after.size, before.size);
    assert!(after.digest.is_none());
    assert!(
        after.modified.as_i64() > before.modified.as_i64(),
        "a rewrite moves the modification time"
    );
}

#[test]
fn walk_records_a_status_change_a_chmod_makes_without_a_modification() {
    // Arrange
    let root = tree_with_a_file("walk_records_a_chmod");
    let path = root.join("etc/greeting");
    let before = entry_at(&walked(&root, &hashing_everything()), "etc/greeting").clone();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("a writable file");

    // Act
    let after = entry_at(&walked(&root, &hashing_everything()), "etc/greeting").clone();

    // Assert: the two stamps are separate facts. A chmod moves the inode's status change
    // time and leaves the content's alone, which is also why a backdating `touch -d` shows
    // up as the two disagreeing.
    assert_eq!(after.modified, before.modified);
    assert!(
        after.changed.as_i64() > before.changed.as_i64(),
        "a permissions change moves the status change time"
    );
}

#[test]
fn walk_records_a_device_nodes_major_and_minor_numbers() {
    // Arrange: a device node cannot be created without root, so the walk is pointed at one
    // the host already has. `/dev/null` is a character device on every Unix.
    let policy = hashing_everything();

    // Act
    let inventory = FileTree::at(Path::new("/dev/null"))
        .walk(&policy)
        .expect("a readable device node");

    // Assert: the numbers are the whole of a device node's state, and nothing is read from
    // it — a hashing policy still leaves a device without a digest.
    let entry = &inventory.entries()[0];
    assert_eq!(entry.kind, FileKind::CharacterDevice);
    assert!(entry.digest.is_none());
    let device = entry.device.expect("a device node carries its numbers");

    // The encoding of `st_rdev` is the kernel's, so the values are asserted where the
    // kernel is Linux. Elsewhere the test still proves the pair is recorded at all.
    #[cfg(target_os = "linux")]
    {
        assert_eq!(device.major(), 1);
        assert_eq!(device.minor(), 3);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = device;
}

#[test]
fn walk_records_a_socket_and_a_fifo_as_having_no_content() {
    // Arrange: a socket comes from `UnixListener`, a fifo needs `mkfifo`, because std
    // offers no way to make one and rastro's own `libc` use stops at constants. The
    // scratch name is short because a bind path has to fit in `sockaddr_un`, which is
    // roughly 100 bytes for the whole absolute path.
    let root = scratch_tree("socket_fifo", &[]);
    UnixListener::bind(root.join("socket")).expect("a bindable scratch path");
    let made = Command::new("mkfifo")
        .arg(root.join("fifo"))
        .status()
        .expect("mkfifo on PATH");
    assert!(made.success(), "mkfifo should have made the fifo");

    // Act: a hashing policy, so this also proves neither is opened. Opening the fifo would
    // block until something wrote to it, and this test would never finish.
    let entries = walked(&root, &hashing_everything());

    // Assert
    for name in ["socket", "fifo"] {
        let entry = entry_at(&entries, name);
        assert!(entry.digest.is_none(), "{name} has no content to digest");
        assert!(entry.size.is_none(), "{name} has no size worth recording");
    }
    assert_eq!(entry_at(&entries, "socket").kind, FileKind::Socket);
    assert_eq!(entry_at(&entries, "fifo").kind, FileKind::Fifo);
}

/// Linux paths are bytes, and a name that is not UTF-8 is legal there. APFS rejects one
/// outright, so the case can only be arranged where it can actually happen.
#[cfg(target_os = "linux")]
#[test]
fn walk_refuses_a_path_that_is_not_utf8_rather_than_substituting() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    // Arrange
    let root = scratch_tree("walk_refuses_a_non_utf8_path", &[]);
    fs::write(root.join(OsStr::from_bytes(b"\xff")), HELLO).expect("a writable tree");

    // Act
    let refused = FileTree::at(&root).walk(&hashing_everything());

    // Assert: substituting `U+FFFD` would put a path in the document that is not on the
    // box and that nobody can act on, which is the refusal `canonical_tool` already makes.
    let message = refused
        .expect_err("a path that will not decode")
        .to_string();
    assert!(
        message.contains("not valid UTF-8"),
        "the refusal should say why, got {message:?}"
    );
}

#[test]
fn an_entry_renders_every_field_it_owns() {
    // Arrange
    let root = tree_with_a_file("entry_renders_every_field");
    let entries = walked(&root, &hashing_everything());

    // Act
    let rendered = Observation::from(entry_at(&entries, "etc/greeting"));

    // Assert: an observation object keeps its keys sorted, so this is both the field list
    // and the order two runs are byte-identical in. Adding a field changes this line,
    // which is the point: the shape is the contract.
    assert_eq!(
        keys_of(&rendered),
        vec![
            "changed_nanoseconds_since_epoch",
            "device",
            "digest",
            "group",
            "inode",
            "kind",
            "link_count",
            "link_target",
            "mode",
            "modified_nanoseconds_since_epoch",
            "owner",
            "size",
        ]
    );
    assert_eq!(text(&field(&rendered, "kind")), "regular");
    assert_eq!(text(&field(&rendered, "mode")), "0644");
    assert_eq!(integer(&field(&rendered, "size")), HELLO.len() as i64);
    assert_eq!(
        text(&field(&field(&rendered, "digest"), "value")),
        HELLO_DIGEST
    );
}

#[test]
fn an_entry_renders_what_it_does_not_have_as_null() {
    // Arrange
    let root = tree_with_a_file("entry_renders_absence");
    let entries = walked(&root, &hashing_everything());

    // Act
    let rendered = Observation::from(entry_at(&entries, "etc"));

    // Assert: the key stays and the value is null, so a diff of two hosts lines the
    // entries up on the same keys whatever kind each path turned out to be.
    for absent in ["size", "link_target", "device", "digest"] {
        assert!(
            is_null(&field(&rendered, absent)),
            "a directory has no {absent}"
        );
    }
}

#[test]
fn an_entry_renders_a_device_as_its_two_numbers() {
    // Arrange
    let inventory = FileTree::at(Path::new("/dev/null"))
        .walk(&hashing_everything())
        .expect("a readable device node");

    // Act
    let rendered = Observation::from(&inventory.entries()[0]);

    // Assert: one leaf per number, under the key the kind gives meaning to.
    let device = field(&rendered, "device");
    assert_eq!(keys_of(&device), vec!["major", "minor"]);
}

#[test]
fn an_inventory_is_keyed_by_path_in_path_order() {
    // Arrange
    let root = scratch_tree("inventory_is_keyed_by_path", &[]);
    write(&root, "zulu", HELLO);
    write(&root, "alpha", HELLO);

    // Act
    let rendered = Observation::from(
        &FileTree::at(&root)
            .walk(&hashing_everything())
            .expect("a readable tree"),
    );

    // Assert: a path is unique, so keying by it loses nothing and removes the ordering
    // churn a list would carry every time an entry appears or goes.
    let expected: Vec<String> = ["", "alpha", "zulu"]
        .iter()
        .map(|name| {
            root.join(name)
                .to_string_lossy()
                .trim_end_matches('/')
                .to_owned()
        })
        .collect();
    assert_eq!(keys_of(&rendered), expected);
}

#[test]
fn walk_refuses_a_root_that_is_not_there() {
    // Act
    let refused = FileTree::at(Path::new("/rastro-no-such-root")).walk(&hashing_everything());

    // Assert: a walk that cannot start is a failure, not an empty tree. An empty
    // inventory would read as a host with no files on it.
    assert!(refused.is_err());
}
