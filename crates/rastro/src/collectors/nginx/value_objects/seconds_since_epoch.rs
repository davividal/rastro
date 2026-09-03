//! An instant, as the document carries one.

use rastro_collector::Observation;

/// Whole seconds since the epoch, UTC.
///
/// **An integer rather than a formatted date**, which is the same call the walk makes for a
/// file's stamps and the timers facet for a schedule. Collectors classify and renderers
/// present: a calendar is presentation, and a fingerprint that carried one would have to
/// pick a spelling and hold it forever.
///
/// Seconds, because a certificate's validity is written in whole seconds and nothing
/// finer exists to record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SecondsSinceEpoch(i64);

impl SecondsSinceEpoch {
    pub fn new(seconds: i64) -> Self {
        Self(seconds)
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl From<&SecondsSinceEpoch> for Observation {
    fn from(instant: &SecondsSinceEpoch) -> Self {
        Observation::integer(instant.as_i64())
    }
}
