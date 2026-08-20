//! Everything scheduled on the box.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::cron_table::CronTable;

/// Every cron source, keyed by where it came from.
///
/// A file path for a crontab or a run-parts directory, and an account name for a user's own
/// crontab out of the spool.
///
/// **A source that is not on the host is present with no table rather than missing**, the rule
/// every inventory here follows: `/etc/crontab` being absent is a fact about the box, and a
/// document silent about it cannot be told from one written before rastro read it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CronState(BTreeMap<String, Option<CronTable>>);

impl CronState {
    pub fn new(
        sources: impl IntoIterator<Item = (String, Option<CronTable>)>,
    ) -> Result<Self, CollectionError> {
        let mut filed = BTreeMap::new();

        for (source, table) in sources {
            if filed.insert(source.clone(), table).is_some() {
                return Err(CollectionError::new(format!("{source:?} was read twice")));
            }
        }

        Ok(Self(filed))
    }

    pub fn sources(&self) -> &BTreeMap<String, Option<CronTable>> {
        &self.0
    }
}

impl From<&CronState> for Observation {
    fn from(state: &CronState) -> Self {
        Observation::object(state.sources().iter().map(|(source, table)| {
            let reported = match table {
                Some(table) => Observation::from(table),
                None => Observation::null(),
            };

            (source.as_str(), reported)
        }))
    }
}
