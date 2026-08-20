//! When a cron job runs.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A job's schedule, as the crontab spells it.
///
/// `17 * * * *`, `5-55/10 * * * *`, `30 3 * * 0`, or one of the shorthands `@reboot`,
/// `@daily`, `@weekly`, `@monthly`, `@yearly`, `@annually`, `@midnight`, `@hourly`.
///
/// **Kept whole rather than split into five typed fields**, and the richness of the syntax is
/// why. A field may be a number, a name (`sun`, `jan`), a list, a range, a range with a step,
/// or `*` with a step, and Vixie cron accepts combinations of all of those. Modelling it would
/// mean reimplementing that grammar to gain nothing a diff needs: the schedule text changes
/// exactly when the schedule changes.
///
/// The five fields are normalised to single spaces, though, because that is transport rather
/// than meaning: crontabs are conventionally aligned with tabs, and `17 *\t* * *` schedules the
/// same job as `17 * * * *`. Without that, reindenting a file would show up as every job
/// changing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Schedule(NonEmptyText);

impl Schedule {
    /// Reads a schedule, collapsing the whitespace between its fields.
    pub fn new(value: &str) -> Result<Self, CollectionError> {
        let normalised = value.split_whitespace().collect::<Vec<&str>>().join(" ");

        Ok(Self(NonEmptyText::new(normalised, "schedule")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this job runs at boot rather than on a clock.
    ///
    /// Worth being able to ask: `@reboot` is the one schedule that makes a job part of how the
    /// box comes up, which puts it in the same class as an enabled unit.
    pub fn is_at_boot(&self) -> bool {
        self.as_str() == "@reboot"
    }
}

impl From<&Schedule> for Observation {
    fn from(schedule: &Schedule) -> Self {
        Observation::text(schedule.as_str())
    }
}
