//! What the running kernel is tuned to.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the
//! last two knows a host interface exists.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::SysctlParameters;
pub use source::{ProcSys, proc_sys_entry};
pub use value_objects::{Readability, SysctlKey, SysctlValue};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct SysctlCollector {
    name: FacetName,
    identity: CollectorIdentity,
    parameters: ProcSys,
}

impl SysctlCollector {
    pub fn new() -> Self {
        Self::reading(ProcSys::new())
    }

    /// The same collector over a source the caller chose.
    ///
    /// The seam that makes [`Self::presence`] testable: all three of its answers
    /// depend on what is on the filesystem, and a test cannot unmount `/proc`.
    pub fn reading(parameters: ProcSys) -> Self {
        Self {
            name: FacetName::new("sysctl").expect("`sysctl` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("sysctl").expect("`sysctl` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            parameters,
        }
    }
}

impl Default for SysctlCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for SysctlCollector {
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
    /// A kernel built without `CONFIG_SYSCTL` publishes no `/proc/sys` and genuinely
    /// has no tunable parameters, so that is `absent` rather than a failure. A host
    /// with no `/proc` at all is neither: rastro cannot see kernel state, and
    /// reporting "no parameters" there would be a confident lie.
    fn presence(&self) -> Presence {
        if self.parameters.exists() {
            return Presence::Present;
        }

        if self.parameters.filesystem_is_mounted() {
            return Presence::Absent;
        }

        Presence::Undetermined {
            reason: format!(
                "{} is not mounted, so whether this kernel has tunable parameters cannot be told",
                self.parameters.filesystem().display()
            ),
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.parameters.read()?))
    }
}
