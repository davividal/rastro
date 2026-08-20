//! What this box has scheduled.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! # Four sources, because cron has four and missing one hides a job
//!
//! - `/etc/crontab`, the system crontab, whose lines carry an account column.
//! - `/etc/cron.d/*`, the same dialect, one file per package.
//! - `/var/spool/cron/crontabs/*`, a user's own crontab, whose lines have **no** account
//!   column because the filename is the account.
//! - `/etc/cron.hourly` and its three siblings, which hold scripts with no schedule at all:
//!   four lines in `/etc/crontab` run them with `run-parts`. **A script dropped in one of
//!   these is a new scheduled job, and no schedule anywhere changed**, so a facet that read
//!   only crontab lines would miss it entirely.
//!
//! The account column is the difference that matters between the first dialect and the third.
//! Reading a user crontab as a system one silently turns the first word of every command into
//! the account the job runs as.
//!
//! systemd timers do the same work on a modern box and are in the `timers` facet. Both exist,
//! and a box can schedule the same thing either way, which is a good reason to read both.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{CronJob, CronState, CronTable};
pub use source::{CronFiles, OwnerColumn, crontab};
pub use value_objects::{CronCommand, JobOwner, Schedule, ScriptName, VariableName};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct CronCollector {
    name: FacetName,
    identity: CollectorIdentity,
    files: CronFiles,
}

impl CronCollector {
    pub fn new() -> Self {
        Self::reading(CronFiles::new())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(files: CronFiles) -> Self {
        Self {
            name: FacetName::new("cron").expect("`cron` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("cron").expect("`cron` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            files,
        }
    }
}

impl Default for CronCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for CronCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present, because the subject is the places cron keeps jobs and rastro can always
    /// report on those.
    ///
    /// A box with cron uninstalled has every source absent, and the data says exactly that.
    /// `absent` on the facet would be the same claim made less precisely, and it would lose
    /// the distinction between "no `/etc/cron.d` at all" and "an empty one".
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.files.read()?))
    }
}
