//! Which account a cron job runs as.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The account a system cron job runs as.
///
/// **The single most security-relevant field in this facet.** A job's owner is the sixth
/// column of `/etc/crontab` and of every file in `/etc/cron.d`, and it is what decides whether
/// a scheduled command runs as `root` or as `nobody`. A job moving to `root` is exactly the
/// kind of change a fingerprint exists to catch, and it is one word in the middle of a line.
///
/// Absent for a job in a user's own crontab, where the account is the file's name rather than
/// a column, and where a user cannot choose it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct JobOwner(NonEmptyText);

impl JobOwner {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "job owner")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&JobOwner> for Observation {
    fn from(owner: &JobOwner) -> Self {
        Observation::text(owner.as_str())
    }
}
