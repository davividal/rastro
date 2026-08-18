//! What is mounted where, and how.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last two
//! knows a host interface exists.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{Mount, MountTable};
pub use source::{ProcMounts, ProcMountsLine};
pub use value_objects::{Device, FilesystemType, MountOption, MountOptions, MountPoint};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct MountsCollector {
    name: FacetName,
    identity: CollectorIdentity,
    table: ProcMounts,
}

impl MountsCollector {
    pub fn new() -> Self {
        Self {
            name: FacetName::new("mounts").expect("`mounts` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("mounts").expect("`mounts` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            table: ProcMounts::new(),
        }
    }
}

impl Default for MountsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for MountsCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present: a running host has mounts, whatever they turn out to be.
    ///
    /// An unreadable table is a failure to read them, never evidence that there are
    /// none, so it surfaces from `collect` as an error rather than as a confident and
    /// wrong `absent`.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.table.read()?))
    }
}
