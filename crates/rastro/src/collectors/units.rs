//! Which systemd units exist, which are enabled, and what they are doing.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **The collector `design.md` names the field research for.** An enablement symlink
//! appearing under a `*.wants/` directory is cited there as exactly what this tool
//! exists to catch, and it surfaces here: enabling a service moves its unit file state
//! from `disabled` to `enabled` and touches nothing else on the box.
//!
//! Two questions get asked, because systemd answers two different ones. `list-unit-files`
//! says what is installed and whether it is enabled; `list-units` says what is loaded
//! and what it is doing. They overlap without containing each other, so both are read
//! and joined by name.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Unit, UnitFile, UnitRegistry, UnitRuntime};
pub use source::{Systemctl, UnitFileRow, UnitRow};
pub use value_objects::{
    ActiveState, Description, LoadState, PresetState, SubState, UnitFileState, UnitName,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct UnitsCollector {
    name: FacetName,
    identity: CollectorIdentity,
    systemctl: Option<Systemctl>,
}

impl UnitsCollector {
    /// Detects systemd once, at construction.
    ///
    /// Detecting here rather than inside `presence` is what stops the two disagreeing:
    /// what was found is the very thing `collect` will read.
    pub fn new() -> Self {
        Self::reading(Systemctl::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(systemctl: Option<Systemctl>) -> Self {
        Self {
            name: FacetName::new("units").expect("`units` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("units").expect("`units` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            systemctl,
        }
    }
}

impl Default for UnitsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for UnitsCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Two answers, and `absent` here is a genuine statement about the host rather than
    /// a hedge.
    ///
    /// This is the opposite call from the packages collector, and the difference is
    /// worth stating because both look like "a tool was not found". There, two negative
    /// probes cannot establish that a box has no packages, because rastro does not ship
    /// a source for rpm. Here there is nothing else to ship: a box either runs systemd
    /// as its init or it does not, and one without `systemctl` has no systemd units in
    /// the same way a kernel without module support has no modules. So `absent` is
    /// exact, and it is diffable in the direction that matters, since a box migrating to
    /// systemd flips the facet from absent to a full registry.
    fn presence(&self) -> Presence {
        match self.systemctl {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let systemctl = self.systemctl.as_ref().ok_or_else(|| {
            CollectionError::new("systemd was not found, so there are no units to read")
        })?;

        Ok(Observation::from(&systemctl.read()?))
    }
}
