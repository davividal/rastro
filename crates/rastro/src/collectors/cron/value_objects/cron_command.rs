//! What a cron job runs.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The command line a job runs, kept verbatim.
///
/// **Not split, not interpreted, and the `%` rule is the reason it cannot be.** cron hands the
/// command to a shell, so it may be an arbitrary pipeline: this box has
/// `test -x /usr/sbin/anacron || { cd / && run-parts --report /etc/cron.daily; }` as one
/// command. It also treats an unescaped `%` specially, cutting the command there and feeding
/// what follows to the job on standard input. Splitting or normalising any of that would
/// change what the fingerprint says the box runs.
///
/// The whole rest of the line after the schedule and the owner, whitespace and all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CronCommand(NonEmptyText);

impl CronCommand {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "cron command")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&CronCommand> for Observation {
    fn from(command: &CronCommand) -> Self {
        Observation::text(command.as_str())
    }
}
