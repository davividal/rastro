//! `ctr tasks ls`: which of a namespace's containers are running, and as what.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::model::ContainerdTask;
use crate::collectors::containers::value_objects::ContainerId;

/// The header `ctr` prints above the rows, which is the one line to skip.
const HEADER: &str = "TASK";

/// How many columns a row has: the container's id, the pid, and the status.
const COLUMNS: usize = 3;

/// The tasks of one namespace, keyed by the container they belong to.
///
/// **The one `ctr` read that parses a table, and it is a considered exception.** Everything
/// else here uses `--quiet` or JSON, but `tasks ls --quiet` prints ids alone and the pid and
/// the status are the whole reason to ask. containerd offers no `tasks info`, so the table
/// is the only place those two exist.
///
/// The columns are split on whitespace, which is safe for exactly these three: an id, a
/// number and a single word, none of which can contain a space. A row with any other number
/// of columns is refused rather than guessed at, because that is the day the format changed.
pub struct CtrTasks(BTreeMap<ContainerId, ContainerdTask>);

impl CtrTasks {
    pub fn parse(output: &str) -> Result<Self, CollectionError> {
        let mut tasks = BTreeMap::new();

        for line in output.lines() {
            let row = line.trim();
            if row.is_empty() || row.starts_with(HEADER) {
                continue;
            }

            let columns: Vec<&str> = row.split_whitespace().collect();
            if columns.len() != COLUMNS {
                return Err(CollectionError::new(format!(
                    "could not read what `ctr tasks ls` reported: the row {row:?} has \
                     {} columns rather than {COLUMNS}",
                    columns.len()
                )));
            }

            let process_id = columns[1].parse::<i64>().map_err(|error| {
                CollectionError::new(format!(
                    "could not read what `ctr tasks ls` reported: {:?} is not a pid: {error}",
                    columns[1]
                ))
            })?;

            tasks.insert(
                ContainerId::new(columns[0])?,
                ContainerdTask {
                    process_id,
                    status: NonEmptyText::new(columns[2], "task status")?,
                },
            );
        }

        Ok(Self(tasks))
    }

    /// The task of one container, or nothing for a container that is not running.
    pub fn of(&self, container: &ContainerId) -> Option<ContainerdTask> {
        self.0.get(container).cloned()
    }
}
