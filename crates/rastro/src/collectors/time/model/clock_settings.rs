//! How the box keeps time.

use rastro_collector::Observation;

use crate::collectors::time::value_objects::Timezone;

/// The host's time configuration.
///
/// **Three facts, all of them stable, and no reading of the current time.** An earlier
/// version of this type carried the system and hardware clocks as volatile values, because
/// `timedatectl` reports them. It no longer asks `timedatectl` anything, for the reason the
/// collector's own documentation gives, and a clock reading was the one thing the files
/// cannot supply. Nothing is lost from the diffable view, because a clock never reached it.
///
/// `local_real_time_clock` is the one worth explaining. When it is false the hardware clock
/// runs in UTC, which is what a server should do; when true it runs in local time, which is
/// what a box dual-booting Windows does and which makes every timestamp ambiguous twice a
/// year.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockSettings {
    /// Absent on a host that has never had one configured, which leaves every program on
    /// it in UTC.
    pub timezone: Option<Timezone>,
    pub local_real_time_clock: bool,
    /// Whether a time-synchronisation service has actually synchronised.
    ///
    /// Not volatile: a box that loses sync and does not regain it is a fault worth seeing
    /// in a diff, not noise.
    pub synchronised: bool,
}

impl From<&ClockSettings> for Observation {
    fn from(settings: &ClockSettings) -> Self {
        Observation::object([
            (
                "local_real_time_clock",
                Observation::boolean(settings.local_real_time_clock),
            ),
            ("synchronised", Observation::boolean(settings.synchronised)),
            (
                "timezone",
                settings
                    .timezone
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
        ])
    }
}
