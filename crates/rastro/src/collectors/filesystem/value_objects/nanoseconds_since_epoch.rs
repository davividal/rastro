//! A moment, as the kernel stamps an inode.

use rastro_collector::Observation;

/// An inode timestamp in nanoseconds since the Unix epoch.
///
/// **The kernel's own resolution, kept rather than rounded.** `st_mtim` is a `timespec`,
/// a second and a nanosecond, and ext4 stores both. Recording only the second would make
/// two writes inside one second indistinguishable, and the format admits no floating
/// point, so a second with a fraction is not on offer. Rendering a calendar date instead
/// would mean rastro being right about every zone and leap-second rule to gain
/// readability and no signal.
///
/// **Not volatile**, which is the difference from a systemd timer's moment: a file that
/// nobody touched carries the same stamp on both runs, so the byte-identical guarantee
/// holds with these in the document. Stamp and lock files whose only churn *is* their
/// mtime are noise in a diff, but they are real changes, and the answer to them is the
/// walker's exclusion scope rather than pretending the value moves on its own.
///
/// **There is no access time here on purpose.** rastro reads a file's content to hash it,
/// which moves that file's atime, so recording atime would report the tool's own visit as
/// a change to the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NanosecondsSinceEpoch(i64);

impl NanosecondsSinceEpoch {
    /// Combines a `timespec`'s two halves, or nothing when the stamp is too far out to hold.
    ///
    /// i64 nanoseconds run out in 2262, so the overflow is only reachable from a stamp no
    /// clock produced. It is refused rather than wrapped, because a wrapped stamp would
    /// read as a plausible date. The caller names the path, which is what makes the refusal
    /// actionable, so the failure is an absent value here rather than a message.
    pub fn of(seconds: i64, nanoseconds: i64) -> Option<Self> {
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|scaled| scaled.checked_add(nanoseconds))
            .map(Self)
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl From<&NanosecondsSinceEpoch> for Observation {
    fn from(moment: &NanosecondsSinceEpoch) -> Self {
        Observation::integer(moment.as_i64())
    }
}
