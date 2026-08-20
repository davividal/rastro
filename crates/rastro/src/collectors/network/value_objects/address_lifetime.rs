//! How long an address has left.

use rastro_collector::Observation;

/// Whether an address is permanent, or leased and counting down.
///
/// **The countdown is pure noise and the fact of being leased is not, which is why this
/// is a type rather than a number.** `ip` reports both lifetimes as a count of seconds. On
/// the development box the DHCP address reads `62922` on one run and less on the next,
/// while a statically configured address reads `4294967295` — `0xFFFFFFFF`, the kernel's
/// sentinel for forever.
///
/// Recording the raw number and marking the field volatile would have thrown away the
/// useful half: an address changing from permanent to leased is a real change in how the
/// box gets its addressing, and it would have been invisible in the diffable view. So the
/// sentinel is decoded here, the boolean is stable, and only the seconds are volatile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AddressLifetime {
    Permanent,
    Leased { remaining_seconds: u32 },
}

/// The kernel's sentinel for a lifetime that never expires.
const FOREVER: u32 = u32::MAX;

impl AddressLifetime {
    pub fn new(seconds: u32) -> Self {
        if seconds == FOREVER {
            return Self::Permanent;
        }

        Self::Leased {
            remaining_seconds: seconds,
        }
    }

    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Permanent)
    }
}

impl From<&AddressLifetime> for Observation {
    fn from(lifetime: &AddressLifetime) -> Self {
        Observation::object([
            ("permanent", Observation::boolean(lifetime.is_permanent())),
            (
                "remaining_seconds",
                match lifetime {
                    // Volatile even when absent, for the reason the timers facet gives:
                    // an address that becomes leased would otherwise show `null` turning
                    // into a number in the diffable view.
                    AddressLifetime::Permanent => Observation::null().volatile(),
                    AddressLifetime::Leased { remaining_seconds } => {
                        Observation::integer(i64::from(*remaining_seconds)).volatile()
                    }
                },
            ),
        ])
    }
}
