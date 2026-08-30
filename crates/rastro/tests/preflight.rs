//! What a run warns about before it starts.

use rastro::preflight::{Estimate, concern};

fn estimate(inodes: u64) -> Estimate {
    Estimate {
        inodes,
        document_bytes: inodes * 81,
    }
}

#[test]
fn a_document_that_fits_easily_draws_no_warning() {
    // Act & Assert: a warning on every run is a warning nobody reads, so this stays quiet
    // until the document is a real fraction of what is free.
    assert_eq!(concern(estimate(50_000), Some(10 * 1_073_741_824)), None);
}

#[test]
fn a_document_that_would_crowd_the_disk_says_so_in_numbers() {
    // Arrange: two million inodes at 81 bytes each is about 162 MB, against 200 MB free.
    let crowded = concern(estimate(2_000_000), Some(200 * 1_048_576));

    // Act & Assert: the operator gets both figures rather than an adjective, because the
    // decision of whether 162 MB matters is theirs.
    let warning = crowded.expect("a document this size against this much free space");
    assert!(warning.contains("2000000"), "got {warning}");
    assert!(warning.contains("154"), "got {warning}");
    assert!(warning.contains("200"), "got {warning}");
}

#[test]
fn nothing_is_claimed_when_the_free_space_is_unknown() {
    // Act & Assert: an absent measurement produces no warning rather than a guessed one. btrfs
    // and zfs report no meaningful inode count, so this path is reached on real hosts.
    assert_eq!(concern(estimate(2_000_000), None), None);
}
