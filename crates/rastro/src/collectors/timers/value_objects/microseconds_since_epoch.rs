//! A moment, as systemd counts them.

use rastro_collector::Observation;

/// An absolute time in microseconds since the Unix epoch.
///
/// **systemd's own unit, kept rather than converted.** `list-timers --output=json`
/// reports `1787236810846290` where the table prints `Thu 2026-08-20 09:20:10 UTC`, and
/// the number is what systemd holds internally. Rendering it as a calendar date would
/// mean rastro being right about every zone and leap-second rule to gain readability and
/// no signal, and the format admits no floating point, so seconds-with-a-fraction is not
/// on offer either.
///
/// **Every value of this type is volatile**, and the type exists partly to make that
/// obvious at each field that has one. A timer's next elapse moves whenever it fires and
/// its last trigger moves with it, so two runs of an unchanged host disagree on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MicrosecondsSinceEpoch(i64);

impl MicrosecondsSinceEpoch {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl From<&MicrosecondsSinceEpoch> for Observation {
    fn from(moment: &MicrosecondsSinceEpoch) -> Self {
        // Volatile at the leaf, so no caller has to remember to mark it.
        Observation::integer(moment.as_i64()).volatile()
    }
}
