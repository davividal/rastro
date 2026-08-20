//! One timer, and when it fires.

use rastro_collector::Observation;

use crate::collectors::systemd::UnitName;
use crate::collectors::timers::value_objects::MicrosecondsSinceEpoch;

/// A timer as rastro means it.
///
/// The timer's own name is not a field here, because it is the key this is filed under.
///
/// **Only one of the four fields survives the diffable view, and that is the honest
/// outcome rather than a shortcoming.** What a timer *is* — that it exists and which
/// unit it starts — is stable, and it is the part a diff should show: a new timer
/// appearing, or an existing one being pointed at a different service, is a real change.
/// When it next fires is a clock, and a clock in a diff is noise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timer {
    /// The unit this timer starts when it fires.
    ///
    /// Absent for a timer with no `Unit=` and no matching service, which systemd reports
    /// with nothing in the column rather than as an error.
    pub activates: Option<UnitName>,
    /// Absent for a timer that will not fire again, such as a `OnCalendar=` whose window
    /// has passed.
    pub next_elapse: Option<MicrosecondsSinceEpoch>,
    /// Absent for a timer that has never fired, which is every timer on a freshly
    /// installed box.
    pub last_trigger: Option<MicrosecondsSinceEpoch>,
}

impl From<&Timer> for Observation {
    fn from(timer: &Timer) -> Self {
        Observation::object([
            (
                "activates",
                timer
                    .activates
                    .as_ref()
                    .map_or_else(Observation::null, |unit| Observation::text(unit.as_str())),
            ),
            (
                "last_trigger",
                timer
                    .last_trigger
                    .as_ref()
                    .map_or_else(null_clock, Observation::from),
            ),
            (
                "next_elapse",
                timer
                    .next_elapse
                    .as_ref()
                    .map_or_else(null_clock, Observation::from),
            ),
        ])
    }
}

/// An absent clock, marked volatile like a present one.
///
/// **The annotation has to be here too, and forgetting it is a real bug rather than a
/// tidiness point.** A timer that has never fired reports no last trigger and then
/// reports one the moment it does. If the absence were stable and the value volatile,
/// the diffable view would show `null` disappearing, which is the very churn the
/// annotation exists to remove.
fn null_clock() -> Observation {
    Observation::null().volatile()
}
