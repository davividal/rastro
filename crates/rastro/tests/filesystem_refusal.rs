//! What the walk does with a path it could not describe.
//!
//! The rule lives here rather than in `filesystem_walk.rs` because the case that matters
//! most cannot be arranged in a real tree: a path vanishing between its parent being listed
//! and it being stat'd is a race, and a test that tried to win it would be flaky. Both the
//! classification and the recording it decides are pure functions for exactly that reason, so
//! each arm is stated directly here and the walk is tested for the failures that *can* be
//! arranged.

use std::io;
use std::io::ErrorKind;
use std::path::Path;

mod support;

use rastro::collectors::filesystem::{
    ContentPolicy, Detail, FileEntry, FileKind, FileMode, FilesystemInventory,
    NanosecondsSinceEpoch, Refusal, UnreadablePath, is_absence,
};
use rastro_collector::{AbsolutePath, NonEmptyText};
use support::observation::{field, keys_of, text};

fn path(value: &str) -> AbsolutePath {
    AbsolutePath::new(value, "walked path").expect("a legal path")
}

fn refused(value: &str, reason: &str) -> UnreadablePath {
    UnreadablePath {
        path: path(value),
        reason: NonEmptyText::new(reason, "a refusal").expect("a legal reason"),
    }
}

/// A directory the walk described, as the minimum a conflict needs on the other side.
fn described(value: &str) -> FileEntry {
    let stamp = NanosecondsSinceEpoch::of(1_700_000_000, 0).expect("a stamp inside the epoch");

    FileEntry {
        path: path(value),
        kind: FileKind::Directory,
        mode: FileMode::of(0o40_755),
        owner: 0,
        group: 0,
        size: None,
        modified: stamp,
        changed: stamp,
        inode: 12,
        link_count: 2,
        link_target: None,
        device: None,
        digest: None,
        reading: ContentPolicy::MetadataOnly,
    }
}

#[test]
fn a_path_that_is_no_longer_there_is_an_absence() {
    // Act & Assert: this is the branch that keeps the byte-identical guarantee true on a busy
    // host. A log rotating away mid-walk must not put an entry in one document and not the
    // next, because that is not a change to the box.
    assert!(is_absence(ErrorKind::NotFound));
}

#[test]
fn a_deleted_file_on_an_nfs_server_is_an_absence_too() {
    // Act & Assert: what an NFS server says about a file removed under an open handle. It
    // means the same thing as `NotFound`, and classifying it otherwise would record a
    // refusal that cannot reproduce on the next run.
    assert!(is_absence(ErrorKind::StaleNetworkFileHandle));
}

#[test]
fn a_path_that_will_not_be_read_is_not_an_absence() {
    // Act & Assert: each of these reproduces at the same path on every run, so it diffs
    // cleanly and belongs in the document as the lasting blind spot it is. `ErrorKind` is
    // `non_exhaustive` and std still maps `EIO` to a kind no stable code can name, so the
    // default has to be "not an absence" — the direction that records too much rather than
    // omitting a path that is really there.
    for persistent in [ErrorKind::PermissionDenied, ErrorKind::NotADirectory] {
        assert!(
            !is_absence(persistent),
            "{persistent:?} is a refusal to record, not a path that went away"
        );
    }
}

#[test]
fn a_path_one_walk_described_and_another_could_not_list_keeps_its_description() {
    // Arrange: the normal case for a mount point, not a contradiction. `/boot/efi` is an entry
    // of the walk of `/`, which stops there and only stats it, and the root of its own walk,
    // which tries to list it. Unprivileged — which is what CI is — the listing fails while the
    // stat had already succeeded, so the same path arrives both described and refused.
    let contested = "/boot/efi";

    // Act
    let inventory = FilesystemInventory::new(
        vec![described(contested)],
        vec![refused(
            contested,
            "could not be listed: Permission denied (os error 13)",
        )],
        Vec::new(),
    )
    .expect("a mount point read twice is not a contradiction");

    // Assert: the description wins, because it is strictly more than the refusal — the
    // attributes were obtained, and that the walk holds no entries beneath it is already what
    // a boundary looks like. Refusing the pair instead cost the entire facet on every
    // unprivileged run, and the determinism harness could not see it: two runs failing the
    // same way are still byte-identical.
    assert_eq!(inventory.entries().len(), 1);
    assert_eq!(inventory.entries()[0].path.as_str(), contested);
    assert!(
        inventory.unreadable().is_empty(),
        "got {:?}",
        inventory.unreadable()
    );
}

#[test]
fn an_inventory_carries_refusals_of_different_paths_side_by_side() {
    // Act
    let inventory = FilesystemInventory::new(
        vec![described("/srv")],
        vec![
            refused(
                "/srv/store",
                "could not be listed: Input/output error (os error 5)",
            ),
            refused(
                "/boot/efi",
                "could not be listed: Permission denied (os error 13)",
            ),
        ],
        Vec::new(),
    )
    .expect("distinct paths are no contradiction");

    // Assert: sorted here rather than left to the order the walk happened to refuse them in,
    // for the same reason the entries are.
    let refusals: Vec<&str> = inventory
        .unreadable()
        .iter()
        .map(|refusal| refusal.path.as_str())
        .collect();
    assert_eq!(refusals, vec!["/boot/efi", "/srv/store"]);
}

#[test]
fn a_refused_path_renders_only_the_reason_it_could_not_be_read() {
    // Arrange
    let inventory = FilesystemInventory::new(
        Vec::new(),
        vec![refused(
            "/srv/store",
            "could not be listed: Permission denied (os error 13)",
        )],
        Vec::new(),
    )
    .expect("one refusal is a legal inventory");

    // Act
    let rendered = inventory.observation(Detail::Summary);

    // Assert: attributes or the reason, never a partial set pretending to be complete. A mode
    // and an owner beside a failed listing would read as a description of a directory nobody
    // could enumerate.
    let entry = field(&rendered, "/srv/store");
    assert_eq!(keys_of(&entry), vec!["error".to_owned()]);
    assert!(
        text(&field(&entry, "error")).contains("Permission denied"),
        "got {entry:?}"
    );
}

#[test]
fn a_refusal_at_a_path_that_is_gone_records_nothing() {
    // Arrange: ENOENT, which is what the walk gets for a file that was listed in its parent
    // and deleted before the walk reached it.
    let vanished = path("/etc/rotated.log");
    let refusal = Refusal::at(
        Path::new("/etc/rotated.log"),
        "could not be read",
        &io::Error::from(ErrorKind::NotFound),
    );

    // Act
    let recorded = UnreadablePath::recorded(&vanished, refusal);

    // Assert: nothing to record. Recording it would put an entry in one document and not the
    // next for a reason that is not a change to the box, which is what byte-identity forbids.
    assert_eq!(recorded, None);
}

#[test]
fn a_refusal_at_a_path_that_is_there_records_the_path_and_the_reason() {
    // Arrange: EACCES, which reproduces at the same path on every run.
    let closed = path("/root/.ssh");
    let refusal = Refusal::at(
        Path::new("/root/.ssh"),
        "could not be listed",
        &io::Error::from(ErrorKind::PermissionDenied),
    );

    // Act
    let recorded = UnreadablePath::recorded(&closed, refusal).expect("a lasting blind spot");

    // Assert
    assert_eq!(recorded.path, closed);
    assert!(
        recorded
            .reason
            .as_str()
            .starts_with("/root/.ssh could not be listed: "),
        "the reason names the path and what was attempted, got {:?}",
        recorded.reason.as_str()
    );
}
