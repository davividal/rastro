//! One scheduled job.

use rastro_collector::Observation;

use crate::collectors::cron::value_objects::{CronCommand, JobOwner, Schedule};

/// A cron job as rastro means it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CronJob {
    pub schedule: Schedule,
    /// Absent for a job in a user's own crontab, where the account is the file's name rather
    /// than a column.
    pub owner: Option<JobOwner>,
    pub command: CronCommand,
}

impl From<&CronJob> for Observation {
    fn from(job: &CronJob) -> Self {
        Observation::object([
            ("command", Observation::from(&job.command)),
            (
                "owner",
                job.owner
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "runs_at_boot",
                Observation::boolean(job.schedule.is_at_boot()),
            ),
            ("schedule", Observation::from(&job.schedule)),
        ])
    }
}
