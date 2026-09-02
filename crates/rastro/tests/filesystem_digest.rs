//! What one entry records: a digest of its metadata rather than a list of it.
//!
//! The document names every path on the box, so its floor is the path strings themselves.
//! Listing eleven attributes per path cost 444 bytes an entry and 13 MB on a container with
//! 31k entries; a digest answers the question the default view exists to answer — did anything
//! about this path change — for a sixth of that.

mod support;

use rastro::collectors::filesystem::{
    CanonicalBytes, ContentPolicy, Detail, DigestAlgorithm, FileEntry, FileKind, FileMode,
    NanosecondsSinceEpoch,
};
use rastro_collector::{AbsolutePath, ByteSize, Xxh3Digest};
use rastro_fingerprint::View;
use support::observation::text;

fn stamp(seconds: i64) -> NanosecondsSinceEpoch {
    NanosecondsSinceEpoch::of(seconds, 0).expect("a stamp inside the epoch")
}

fn entry(kind: FileKind, reading: ContentPolicy) -> FileEntry {
    FileEntry {
        path: AbsolutePath::new("/etc/ssh/sshd_config", "walked path").expect("a legal path"),
        kind,
        mode: FileMode::of(0o100_644),
        owner: 0,
        group: 0,
        size: Some(ByteSize::new(3298, "file size").expect("a legal size")),
        modified: stamp(1_700_000_000),
        changed: stamp(1_700_000_000),
        inode: 70365,
        link_count: 1,
        link_target: None,
        device: None,
        digest: None,
        reading,
    }
}

fn regular() -> FileEntry {
    entry(FileKind::Regular, ContentPolicy::MetadataOnly)
}

fn rendered(entry: &FileEntry) -> String {
    let observation = entry.observation(Detail::Summary);
    let visible = observation
        .in_view(View::Diffable)
        .expect("an entry is not wholly volatile");

    text(&visible)
}

#[test]
fn an_entry_renders_as_one_digest_of_its_metadata() {
    // Act
    let digest = rendered(&regular());

    // Assert: sixteen hex characters, which is the whole entry. Not an object with one key:
    // in the default view the digest *is* the entry's value, and a wrapper would cost 45
    // bytes an entry to say nothing.
    assert_eq!(digest.len(), 16, "got {digest:?}");
    assert!(
        digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()),
        "got {digest:?}"
    );
}

#[test]
fn an_entry_whose_mode_changed_gets_a_different_digest() {
    // Arrange
    let before = regular();
    let mut after = regular();
    after.mode = FileMode::of(0o100_600);

    // Act & Assert: this is the whole point. A chmod has to move the digest, or the default
    // view stops answering the only question it is there to answer.
    assert_ne!(rendered(&before), rendered(&after));
}

#[test]
fn every_stable_attribute_moves_the_digest() {
    // Arrange & Act & Assert: one case per attribute, because a digest that silently ignored
    // one would report a real change as no change, and nothing else in the default view would
    // catch it.
    let baseline = rendered(&regular());

    let mut owner = regular();
    owner.owner = 1000;
    assert_ne!(rendered(&owner), baseline, "owner");

    let mut group = regular();
    group.group = 1000;
    assert_ne!(rendered(&group), baseline, "group");

    let mut size = regular();
    size.size = Some(ByteSize::new(3299, "file size").expect("a legal size"));
    assert_ne!(rendered(&size), baseline, "size");

    let mut modified = regular();
    modified.modified = stamp(1_700_000_001);
    assert_ne!(rendered(&modified), baseline, "modified");

    let mut changed = regular();
    changed.changed = stamp(1_700_000_001);
    assert_ne!(rendered(&changed), baseline, "changed");

    let mut inode = regular();
    inode.inode = 70366;
    assert_ne!(rendered(&inode), baseline, "inode");

    let mut link_count = regular();
    link_count.link_count = 2;
    assert_ne!(rendered(&link_count), baseline, "link_count");

    let mut kind = regular();
    kind.kind = FileKind::Fifo;
    assert_ne!(rendered(&kind), baseline, "kind");
}

#[test]
fn a_symlinks_target_moves_the_digest() {
    // Arrange: on a symlink the target *is* the state, and an enablement link under a
    // `*.wants/` directory being repointed is exactly what this walker exists to catch.
    let mut before = entry(FileKind::Symlink, ContentPolicy::MetadataOnly);
    before.link_target = Some("/lib/systemd/system/nginx.service".to_owned());
    let mut after = before.clone();
    after.link_target = Some("/lib/systemd/system/apache2.service".to_owned());

    // Act & Assert
    assert_ne!(rendered(&before), rendered(&after));
}

#[test]
fn a_directorys_derived_stamps_stay_out_of_its_digest() {
    // Arrange: a directory's stamps and link count summarise entries the walk reports one by
    // one, so they move whenever a child appears. Folding them into the digest would put that
    // churn back into the default view, which is what PR 3 removed.
    let before = entry(FileKind::Directory, ContentPolicy::MetadataOnly);
    let mut after = before.clone();
    after.modified = stamp(1_800_000_000);
    after.changed = stamp(1_800_000_000);
    after.link_count = 9;

    // Act & Assert
    assert_eq!(rendered(&before), rendered(&after));
}

#[test]
fn a_churning_trees_moving_attributes_stay_out_of_its_digest() {
    // Arrange: `/var/log` and the rest churn without meaning, so their size, inode and both
    // stamps are volatile. A digest over them would break the byte-identical guarantee on
    // every run, which is the contract everything else rests on.
    let before = entry(FileKind::Regular, ContentPolicy::Churns);
    let mut after = before.clone();
    after.size = Some(ByteSize::new(999_999, "file size").expect("a legal size"));
    after.inode = 999;
    after.modified = stamp(1_900_000_000);
    after.changed = stamp(1_900_000_000);

    // Act & Assert
    assert_eq!(rendered(&before), rendered(&after));
}

#[test]
fn a_churning_entry_still_reports_what_does_not_churn() {
    // Arrange
    let before = entry(FileKind::Regular, ContentPolicy::Churns);
    let mut after = before.clone();
    after.mode = FileMode::of(0o100_600);

    // Act & Assert: churning withholds the attributes that move on their own, not the entry.
    // A chmod under `/var/log` is still a change to the box.
    assert_ne!(rendered(&before), rendered(&after));
}

#[test]
fn a_digest_is_the_same_on_every_run_of_the_same_binary() {
    // Act & Assert: the digest is part of the format contract, so a stored fingerprint stays
    // comparable. A seeded or randomised hash would invalidate every archive on a rebuild,
    // which is why `DefaultHasher` is disqualified whatever its speed.
    assert_eq!(rendered(&regular()), rendered(&regular()));
}

#[test]
fn a_digest_is_written_as_sixteen_lowercase_hex_characters() {
    // Act
    let digest = Xxh3Digest::of(&[0]);

    // Act & Assert: zero-padded, because a digest that varied in width would sort and diff
    // inconsistently between entries.
    assert_eq!(digest.as_str().len(), 16);
    assert_eq!(digest.as_str(), digest.as_str().to_lowercase());
}

#[test]
fn the_content_digest_algorithm_is_still_named_for_the_collector_that_will_use_it() {
    // Act & Assert: nothing hashes content by default any more, but the seam stays for the
    // opt-in collector that will, and its algorithm is named in the document rather than
    // assumed because a sha256 fingerprint cannot be diffed against any other kind.
    assert_eq!(DigestAlgorithm::Sha256.as_str(), "sha256");
}

#[test]
fn a_withheld_text_is_not_the_same_as_an_absent_one() {
    // Arrange: `maybe_text` has to keep the two apart for the same reason `maybe_integer` does.
    // A value the view withholds and a value the host does not have are different facts, and a
    // digest that collapsed them would make a churning symlink and a plain file collide.
    let withheld = CanonicalBytes::new()
        .maybe_text(true, Some("/var/log/nginx"))
        .digest();
    let absent = CanonicalBytes::new().maybe_text(false, None).digest();
    let present = CanonicalBytes::new()
        .maybe_text(false, Some("/var/log/nginx"))
        .digest();

    // Assert
    assert_ne!(withheld.as_str(), absent.as_str());
    assert_ne!(withheld.as_str(), present.as_str());
    assert_ne!(absent.as_str(), present.as_str());
}

#[test]
fn a_withheld_text_says_nothing_about_the_value_it_withheld() {
    // Act & Assert: two different withheld values digest the same, which is what makes the
    // digest stable across runs for a tree whose contents churn.
    assert_eq!(
        CanonicalBytes::new()
            .maybe_text(true, Some("one"))
            .digest()
            .as_str(),
        CanonicalBytes::new()
            .maybe_text(true, Some("another"))
            .digest()
            .as_str()
    );
}
