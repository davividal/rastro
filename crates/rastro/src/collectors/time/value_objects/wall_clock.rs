//! A moment as `timedatectl` renders it.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A rendered timestamp, such as `Thu 2026-08-20 09:43:44 UTC`.
///
/// **Every value of this type is volatile**, and the type exists to make that obvious
/// wherever one appears: it is a reading of a clock, so two runs two seconds apart differ
/// by two seconds. Measured, not assumed — a pair of `timedatectl show` runs on the
/// development box differed in exactly these two fields and nothing else.
///
/// Text rather than a number, despite systemd calling the fields `TimeUSec` and
/// `RTCTimeUSec`. What `timedatectl show` actually prints there is a formatted date, not the
/// microsecond count the name promises, and rastro records what the tool wrote.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WallClock(NonEmptyText);

impl WallClock {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "clock reading")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&WallClock> for Observation {
    fn from(clock: &WallClock) -> Self {
        Observation::text(clock.as_str()).volatile()
    }
}
