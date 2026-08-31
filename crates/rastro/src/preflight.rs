//! What this run is about to cost, guessed before it starts.
//!
//! **An estimate and a warning, not a budget.** A limit an operator has to tune presupposes
//! they have already investigated the box, which is the work rastro exists to do. So nothing
//! here refuses or truncates a run: it tells the operator what is coming, in time to change
//! their mind.

use crate::collectors::canonical_tool::CanonicalTool;

/// Bytes of document per walked path, measured rather than guessed.
///
/// 81 bytes on a Debian container of 30,891 entries, pretty-printed, one digest per path. It
/// moves with the format, so it is an order of magnitude rather than a promise — which is all
/// an estimate needs to be to answer "will this fill the disk".
const BYTES_PER_ENTRY: u64 = 81;

/// What a walk is likely to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estimate {
    pub inodes: u64,
    pub document_bytes: u64,
}

/// An upper bound on the entries this run will walk, from the inodes in use.
///
/// **An over-estimate on purpose, and by an unbounded margin.** `df -i` counts every inode on
/// a filesystem, including the ones under a tree a collector sealed — a PostgreSQL cluster of
/// two million files inflates this and the walk will never touch them. Over-estimating is the
/// right direction for a warning: it can cry wolf, where an under-estimate would stay quiet
/// about the run that fills the disk.
///
/// `None` only where `df` is not on the box, which gets no guess rather than a made-up one.
pub fn estimate() -> Option<Estimate> {
    let inodes = inodes_in_use()?;

    Some(Estimate {
        inodes,
        document_bytes: inodes.saturating_mul(BYTES_PER_ENTRY),
    })
}

/// The warning an operator should see, or nothing when there is nothing to warn about.
///
/// Only when the document would be a substantial fraction of what is free where it is going:
/// a warning on every run is a warning nobody reads.
pub fn concern(estimate: Estimate, free_bytes: Option<u64>) -> Option<String> {
    let free = free_bytes?;
    if estimate.document_bytes.saturating_mul(4) < free {
        return None;
    }

    Some(format!(
        "this host has {} inodes in use, so the document may reach about {} MB against {} MB \
         free where it is being written",
        estimate.inodes,
        estimate.document_bytes / 1_048_576,
        free / 1_048_576
    ))
}

/// Bytes free where a document is about to be written.
///
/// `None` when `df` cannot say, which is the same answer an absent `df` gives: no number, so
/// no warning, rather than a warning built on a guess.
pub fn free_bytes_at(path: &std::path::Path) -> Option<u64> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let target = directory.unwrap_or(std::path::Path::new("."));

    let df = CanonicalTool::located("df")?;

    parse_free_bytes(&df.run(&["-P", "-k", target.to_str()?]).ok()?)
}

/// The available-kilobytes column of one `df -P -k` row, as bytes.
///
/// Separate from the run for the reason every other source in this tree separates them: the
/// whole translation is then exercised from a fixture, with no `df` to run and no disk whose
/// free space the test would have to arrange.
pub fn parse_free_bytes(reported: &str) -> Option<u64> {
    reported
        .lines()
        .nth(1)?
        .split_whitespace()
        .nth(3)?
        .parse::<u64>()
        .ok()
        .map(|kilobytes| kilobytes.saturating_mul(1024))
}

/// Inodes in use across the filesystems `df` reports, kernel interfaces aside.
///
/// Through the hardened `canonical_tool` seam rather than `statfs`, which std does not wrap:
/// reaching that syscall would cost `#![deny(unsafe_code)]` for a number that is only ever a
/// warning. Shelling out to `df` is bounded in time and output and leaves the box as it found
/// it, which the syscall's cost does not buy anything over.
fn inodes_in_use() -> Option<u64> {
    let df = CanonicalTool::located("df")?;

    Some(parse_inodes_in_use(&df.run(&["-i", "-P", "--local"]).ok()?))
}

/// The used-inode columns of `df -i -P --local`, summed.
///
/// **A row that reports no count contributes nothing, and the sum carries on.** vfat has no
/// fixed inode table, so `/boot/efi` prints `0 0 0 -` and the `-` will not parse. Skipping that
/// row is the whole tolerance this needs: `--local` always lists `/`, and on a real box udev and
/// several tmpfs besides, every one of which reports a count. A host that could run rastro
/// therefore never sums to zero, which is why there is no special case for it here — an
/// unreachable branch is complexity, not safety.
///
/// Separate from the run so a fixture can state a `df` this repo has no filesystem to produce.
pub fn parse_inodes_in_use(reported: &str) -> u64 {
    reported
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().nth(2))
        .filter_map(|used| used.parse::<u64>().ok())
        .sum()
}
