//! Which kernel modules are loaded, and what holds them there.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the
//! last two knows a host interface exists.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{KernelModule, ModuleTable};
pub use source::{ProcModules, ProcModulesLine};
pub use value_objects::{
    Dependants, ModuleName, ModuleState, ReferenceCount, Removability, TaintFlag, TaintFlags,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct ModulesCollector {
    name: FacetName,
    identity: CollectorIdentity,
    table: ProcModules,
}

impl ModulesCollector {
    pub fn new() -> Self {
        Self::reading(ProcModules::new())
    }

    /// The same collector over a source the caller chose.
    ///
    /// The seam that makes [`Self::presence`] testable: all three of its answers depend
    /// on what is on the filesystem, and a test cannot unmount `/proc`.
    pub fn reading(table: ProcModules) -> Self {
        Self {
            name: FacetName::new("modules").expect("`modules` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("modules").expect("`modules` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            table,
        }
    }
}

impl Default for ModulesCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for ModulesCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Three answers, because the host really does have three cases.
    ///
    /// A kernel built without `CONFIG_MODULES` publishes no `/proc/modules` and
    /// genuinely has no modules, so that is `absent` rather than a failure. A host with
    /// no `/proc` at all is neither: rastro cannot see kernel state, and saying "no
    /// modules" there would be a confident lie, so it says it could not tell.
    fn presence(&self) -> Presence {
        if self.table.exists() {
            return Presence::Present;
        }

        if self.table.filesystem_is_mounted() {
            return Presence::Absent;
        }

        Presence::Undetermined {
            reason: format!(
                "{} is not mounted, so whether this kernel has modules cannot be told",
                self.table.filesystem().display()
            ),
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.table.read()?))
    }
}
