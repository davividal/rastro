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
    // Act & Assert: an absent measurement produces no warning rather than a guessed one. btrfs
    // and zfs report no meaningful inode count, so this path is reached on real hosts.
    assert_eq!(concern(estimate(2_000_000), None), None);
}

/// `df -i -P --local` as the reference box prints it. Note `/boot/efi`, which is vfat and
/// reports no inode count at all — the case that makes the absent answer necessary.
const INODES: &str = "\
Filesystem      Inodes  IUsed   IFree IUse% Mounted on
/dev/sda1      6545408  49704 6495704    1% /
/dev/sda15           0      0       0     - /boot/efi
";

#[test]
fn the_estimate_sums_the_used_inodes_of_every_filesystem() {
    // Act & Assert: the vfat row contributes nothing rather than breaking the sum.
    assert_eq!(preflight::parse_inodes_in_use(INODES), Some(49_704));
}

#[test]
fn a_host_whose_filesystems_report_no_inodes_gets_no_estimate() {
    // Arrange: every row is a filesystem with no fixed inode table. vfat prints `0`, and btrfs
    // and zfs print `-`, because there is no inode count to report — not because the box has no
    // files on it.
    let no_counts = "\
Filesystem      Inodes  IUsed   IFree IUse% Mounted on
/dev/sda15           0      0       0     - /boot/efi
/dev/nvme0n1p2       -      -       -     - /
";

    // Act & Assert: absent rather than zero. An estimate of nothing is not an estimate of a
    // small number, and reporting zero would compare the document against a floor of zero and
    // warn on every run of such a host.
    assert_eq!(preflight::parse_inodes_in_use(no_counts), None);
}

#[test]
fn a_df_that_printed_only_its_header_gets_no_estimate() {
    // Act & Assert: nothing to sum is the same answer as nothing countable.
    assert_eq!(
        preflight::parse_inodes_in_use("Filesystem Inodes IUsed IFree IUse% Mounted on\n"),
        None
    );
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
