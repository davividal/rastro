//! A script in one of cron's run-parts directories.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The name of a script in `/etc/cron.hourly` and its siblings.
///
/// A name rather than a path, because the directory is the key it is filed under.
///
/// **These directories are jobs without schedules of their own.** Nothing in them says when it
/// runs; `/etc/crontab` says, with four lines that call `run-parts` on each directory. So a
/// script appearing here is a new scheduled job even though no schedule changed anywhere, and
/// a facet that only read crontab lines would miss it completely.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScriptName(NonEmptyText);

impl ScriptName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "script name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ScriptName> for Observation {
    fn from(name: &ScriptName) -> Self {
        Observation::text(name.as_str())
    }
}
