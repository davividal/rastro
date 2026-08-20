//! Which systemd timers exist, and what they start.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **A small facet, and honest about being small.** Of the four things systemd reports
//! about a timer, three are clocks, so the diffable view keeps one: which unit each timer
//! starts. That is still the question a diff needs answered — a new timer, or an existing
//! one repointed at a different service, is a real change — but it is worth saying
//! plainly that a `--include-volatile` run is where the schedule itself shows up.
//!
//! Timers also appear in the `units` facet, because a timer is a unit: `foo.timer` has an
//! enablement state and a load state there. This facet adds the part `systemctl
//! list-unit-files` cannot answer, which is what the timer points at.
pub mod model;
pub mod source;
pub mod value_objects;

pub use crate::collectors::systemd::UnitName;
pub use model::{Timer, TimerTable};
pub use source::{SystemctlTimers, TimerRow};
pub use value_objects::MicrosecondsSinceEpoch;

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct TimersCollector {
    name: FacetName,
    identity: CollectorIdentity,
    timers: Option<SystemctlTimers>,
}

impl TimersCollector {
    pub fn new() -> Self {
        Self::reading(SystemctlTimers::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(timers: Option<SystemctlTimers>) -> Self {
        Self {
            name: FacetName::new("timers").expect("`timers` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("timers").expect("`timers` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            timers,
        }
    }
}

impl Default for TimersCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for TimersCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `absent` when the host does not run systemd, for the reason the units collector
    /// gives: there is no second implementation of a systemd timer that rastro might have
    /// shipped a source for and did not.
    ///
    /// A systemd box with no timers at all is `present` with an empty table, which is a
    /// different statement and a true one.
    fn presence(&self) -> Presence {
        match self.timers {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let timers = self.timers.as_ref().ok_or_else(|| {
            CollectionError::new("systemd was not found, so there are no timers to read")
        })?;

        Ok(Observation::from(&timers.read()?))
    }
}
