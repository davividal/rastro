//! What a run warns about before it starts.

use rastro::preflight::{self, Estimate, concern};

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
    // Act & Assert: an absent measurement produces no warning rather than a guessed one. `df`
    // reaches this whenever it cannot answer for the output path at all.
    assert_eq!(concern(estimate(2_000_000), None), None);
}

/// `df -i -P --local` as the reference Debian box prints it, trimmed to the two rows that
/// matter: the root filesystem, and `/boot/efi`, which is vfat and reports `-` because it has
/// no fixed inode table.
const INODES: &str = "\
Filesystem      Inodes  IUsed   IFree IUse% Mounted on
udev           5998310    347 5997963    1% /dev
/dev/sda1      6545408  49704 6495704    1% /
/dev/sda15           0      0       0     - /boot/efi
";

#[test]
fn a_filesystem_that_reports_no_inode_count_does_not_break_the_sum() {
    // Act & Assert: the `-` row is skipped rather than parsed as zero or refused. This is the
    // whole tolerance the estimate needs — `--local` always lists `/` and the tmpfs mounts, so
    // every host that can run rastro contributes a count.
    assert_eq!(preflight::parse_inodes_in_use(INODES), 347 + 49_704);
}

#[test]
fn free_space_is_read_from_the_available_column() {
    // Arrange: `df -P -k`, whose fourth column is available kilobytes.
    let reported = "\
Filesystem     1024-blocks    Used Available Capacity Mounted on
/dev/sda1         61608176 8090096  50354688      14% /
";

    // Act & Assert
    assert_eq!(
        preflight::parse_free_bytes(reported),
        Some(50_354_688 * 1024)
    );
}

#[test]
fn free_space_is_absent_when_df_named_no_filesystem() {
    // Act & Assert: a header and nothing under it means `df` could not answer for that path,
    // which is not the same as a path with no space.
    assert_eq!(
        preflight::parse_free_bytes("Filesystem 1024-blocks Used Available Capacity Mounted on\n"),
        None
    );
}

#[test]
fn a_document_that_would_crowd_a_tiny_disk_warns_whatever_the_host_actually_has() {
    // Arrange: the two readings a run takes, as values. This is the whole decision — the estimate
    // in inodes and the free space at the destination — and it is the reason neither number is
    // read inside `concern`: a test can state a nearly-full disk that no CI runner has.
    let crowded = estimate(2_000_000);
    let barely_enough = 200 * 1_048_576;

    // Act & Assert: it fires below the four-times margin and stays quiet above it, so the
    // threshold itself is pinned rather than inferred from one run.
    assert!(concern(crowded, Some(barely_enough)).is_some());
    assert!(concern(crowded, Some(barely_enough * 8)).is_none());
}
