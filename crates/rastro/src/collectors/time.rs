//! How this box keeps time.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **A small facet carrying one very large fact.** The timezone moves every log timestamp,
//! every cron schedule and every `OnCalendar=` timer on the box, and it is one line in one
//! file. The two clock readings systemd also reports are volatile and do not reach the
//! diffable view; everything else here is configuration.
//!
//! Kept apart from the `locale` facet, which the same `systemd` family of tools answers for.
//! They are two state surfaces: a facet named `time` holding a keyboard layout would be
//! badly named, and an operator may reasonably want one without the other.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::ClockSettings;
pub use source::Timedatectl;
pub use value_objects::{Timezone, WallClock};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct TimeCollector {
    name: FacetName,
    identity: CollectorIdentity,
    timedatectl: Option<Timedatectl>,
}

impl TimeCollector {
    pub fn new() -> Self {
        Self::reading(Timedatectl::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(timedatectl: Option<Timedatectl>) -> Self {
        Self {
            name: FacetName::new("time").expect("`time` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("time").expect("`time` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            timedatectl,
        }
    }
}

impl Default for TimeCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for TimeCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `undetermined` without the tool, because a box with no `timedatectl` still has a
    /// timezone and a hardware clock. rastro simply cannot see them through this interface,
    /// and `absent` would deny that the box keeps time at all.
    fn presence(&self) -> Presence {
        match self.timedatectl {
            Some(_) => Presence::Present,
            None => Presence::Undetermined {
                reason: "`timedatectl` was not found, so this host's timekeeping cannot be told"
                    .to_owned(),
            },
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let timedatectl = self
            .timedatectl
            .as_ref()
            .ok_or_else(|| CollectionError::new("`timedatectl` was not found"))?;

        Ok(Observation::from(&timedatectl.read()?))
    }
}
