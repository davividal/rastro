//! Every timer systemd has.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::timer::Timer;
use crate::collectors::systemd::UnitName;

/// The timers, keyed by unit name.
///
/// Keyed rather than listed, as everywhere systemd is the source: it enforces one unit
/// per name, so keying loses nothing and removes the order `list-timers` happened to
/// use, which is by next elapse and therefore reshuffles every time one fires.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimerTable(BTreeMap<UnitName, Timer>);

impl TimerTable {
    /// Files each timer under its name, refusing a repeat.
    pub fn new(
        timers: impl IntoIterator<Item = (UnitName, Timer)>,
    ) -> Result<Self, CollectionError> {
        let mut table = BTreeMap::new();

        for (name, timer) in timers {
            if table.insert(name.clone(), timer).is_some() {
                return Err(CollectionError::new(format!(
                    "systemd reported the timer {:?} twice, so the output was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(table))
    }

    pub fn timers(&self) -> &BTreeMap<UnitName, Timer> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&TimerTable> for Observation {
    fn from(table: &TimerTable) -> Self {
        Observation::object(
            table
                .timers()
                .iter()
                .map(|(name, timer)| (name.as_str(), Observation::from(timer))),
        )
    }
}
