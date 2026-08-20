//! How the box keeps time.

use rastro_collector::Observation;

use crate::collectors::time::value_objects::{Timezone, WallClock};

/// The host's time configuration.
///
/// **Four booleans and a timezone survive the diffable view; the two clock readings do
/// not.** That split is the whole facet: how the box is *configured* to keep time is state,
/// and what time it currently thinks it is is not.
///
/// `local_real_time_clock` is the one worth explaining. When it is false the hardware clock
/// runs in UTC, which is what a server should do; when true it runs in local time, which is
/// what a box dual-booting Windows does and which makes every timestamp ambiguous twice a
/// year.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockSettings {
    pub timezone: Timezone,
    pub local_real_time_clock: bool,
    /// Whether a time-synchronisation service is available at all.
    pub can_synchronise: bool,
    /// Whether it is switched on.
    pub synchronisation_enabled: bool,
    /// Whether it has actually synchronised. Not volatile: a box that loses sync and does
    /// not regain it is a fault worth seeing in a diff, not noise.
    pub synchronised: bool,
    pub system_clock: WallClock,
    pub hardware_clock: WallClock,
}

impl From<&ClockSettings> for Observation {
    fn from(settings: &ClockSettings) -> Self {
        Observation::object([
            (
                "can_synchronise",
                Observation::boolean(settings.can_synchronise),
            ),
            (
                "hardware_clock",
                Observation::from(&settings.hardware_clock),
            ),
            (
                "local_real_time_clock",
                Observation::boolean(settings.local_real_time_clock),
            ),
            ("synchronised", Observation::boolean(settings.synchronised)),
            (
                "synchronisation_enabled",
                Observation::boolean(settings.synchronisation_enabled),
            ),
            ("system_clock", Observation::from(&settings.system_clock)),
            ("timezone", Observation::from(&settings.timezone)),
        ])
    }
}
