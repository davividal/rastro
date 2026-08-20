//! One row of `systemctl list-timers --all --output=json`.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::systemd::UnitName;
use crate::collectors::timers::model::Timer;
use crate::collectors::timers::value_objects::MicrosecondsSinceEpoch;

/// systemd's spelling of a timer, kept apart from rastro's meaning.
///
/// **Four time fields arrive and two are recorded, because in this output the other two
/// are copies.** The table `list-timers` prints has `NEXT`, `LEFT`, `LAST` and `PASSED`,
/// where `LEFT` and `PASSED` are durations relative to now. Asked for JSON, systemd 252
/// fills all four with absolute microsecond timestamps, so `left` repeats `next` and
/// `passed` repeats `last` exactly. That was measured on the development box, where every
/// row had `next == left` and `last == passed`.
///
/// Recording all four would put the same number in the document twice under two names
/// that promise different things, which is worse than dropping two: a reader would take
/// `left` for a duration and be wrong by fifty-six years.
#[derive(Debug, Clone, Deserialize)]
pub struct TimerRow {
    unit: String,
    /// Empty when the timer starts nothing systemd can name.
    #[serde(default)]
    activates: String,
    /// `null`, or absent, for a timer that will not fire again.
    #[serde(default)]
    next: Option<i64>,
    /// `null`, or absent, for a timer that has never fired.
    #[serde(default)]
    last: Option<i64>,
    /// Read and deliberately unused: see the note on this type. Named so that a reader
    /// comparing this struct against systemd's output does not think the field was
    /// missed.
    #[serde(default, rename = "left")]
    _left_repeats_next: Option<i64>,
    #[serde(default, rename = "passed")]
    _passed_repeats_last: Option<i64>,
}

impl TimerRow {
    /// Translates systemd's row into rastro's model.
    pub fn to_timer(&self) -> Result<(UnitName, Timer), CollectionError> {
        let timer = Timer {
            activates: optional_unit(&self.activates)?,
            next_elapse: self.next.map(MicrosecondsSinceEpoch::new),
            last_trigger: self.last.map(MicrosecondsSinceEpoch::new),
        };

        Ok((UnitName::new(self.unit.clone())?, timer))
    }
}

/// The unit a timer starts, or nothing when systemd named none.
fn optional_unit(activates: &str) -> Result<Option<UnitName>, CollectionError> {
    if activates.is_empty() {
        return Ok(None);
    }

    Ok(Some(UnitName::new(activates)?))
}
