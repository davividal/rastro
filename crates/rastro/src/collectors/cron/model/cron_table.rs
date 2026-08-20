//! One crontab.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::cron_job::CronJob;
use crate::collectors::cron::value_objects::{ScriptName, VariableName};

/// What one crontab file, or one run-parts directory, contributes.
///
/// **Two shapes behind one type, because cron has two kinds of source and both are jobs.** A
/// crontab holds environment assignments and scheduled lines; a `cron.daily` directory holds
/// scripts with no schedule of their own, run by a line in `/etc/crontab`. Reporting only the
/// first would miss a new script in the second, which is a new scheduled job.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CronTable {
    /// Variables the file sets for every job in it. `PATH` decides which binary a bare
    /// command name resolves to and `MAILTO` decides whether anybody hears about a failure,
    /// so neither is decoration.
    pub environment: BTreeMap<VariableName, String>,
    /// In the file's order, which is not a schedule but is how an operator reads it.
    pub jobs: Vec<CronJob>,
    /// Sorted. Only a run-parts directory has these.
    pub scripts: Vec<ScriptName>,
}

impl CronTable {
    /// A crontab's environment and jobs.
    ///
    /// A repeated variable is refused: cron takes the last, so the file is ambiguous about
    /// what every job in it runs with, and resolving that quietly would hide it.
    pub fn crontab(
        environment: impl IntoIterator<Item = (VariableName, String)>,
        jobs: impl IntoIterator<Item = CronJob>,
    ) -> Result<Self, CollectionError> {
        let mut assigned = BTreeMap::new();
        for (name, value) in environment {
            if assigned.insert(name.clone(), value).is_some() {
                return Err(CollectionError::new(format!(
                    "{:?} is set twice, so what every job in this file runs with depends on \
                     which line cron takes",
                    name.as_str()
                )));
            }
        }

        Ok(Self {
            environment: assigned,
            jobs: jobs.into_iter().collect(),
            scripts: Vec::new(),
        })
    }

    /// A run-parts directory's scripts.
    pub fn scripts(scripts: impl IntoIterator<Item = ScriptName>) -> Self {
        let mut sorted: Vec<ScriptName> = scripts.into_iter().collect();
        sorted.sort();

        Self {
            environment: BTreeMap::new(),
            jobs: Vec::new(),
            scripts: sorted,
        }
    }
}

impl From<&CronTable> for Observation {
    fn from(table: &CronTable) -> Self {
        Observation::object([
            (
                "environment",
                Observation::object(
                    table
                        .environment
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value.clone()))),
                ),
            ),
            (
                "jobs",
                Observation::list(table.jobs.iter().map(Observation::from)),
            ),
            (
                "scripts",
                Observation::list(table.scripts.iter().map(Observation::from)),
            ),
        ])
    }
}
