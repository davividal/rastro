//! Which timezone the box is set to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The configured timezone, as systemd names it.
///
/// `Etc/UTC`, `Europe/Berlin`, `America/Sao_Paulo` — a zoneinfo identifier rather than an
/// offset, which is the distinction that matters: an offset changes twice a year under
/// daylight saving while the identifier does not, so recording the offset would put a
/// seasonal change into a fingerprint.
///
/// **Among the most consequential single values on a server.** Every log timestamp, every
/// cron schedule and every `OnCalendar=` timer moves when this does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timezone(NonEmptyText);

impl Timezone {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "timezone")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&Timezone> for Observation {
    fn from(timezone: &Timezone) -> Self {
        Observation::text(timezone.as_str())
    }
}
