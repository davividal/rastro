//! What storage this box has, and what is on it.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **The companion of the `mounts` facet, and it answers the question that one cannot.**
//! `mounts` reports what the kernel has mounted; this reports every device the box has,
//! whether anything mounted it or not. A partition holding a filesystem that nothing mounts
//! appears only here, and a filesystem UUID changing under a device that kept its name and
//! its size is how a reformat shows up.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{BlockDevice, DeviceTree};
pub use source::Lsblk;
pub use value_objects::{
    DeviceLabel, DeviceModel, DeviceName, DeviceType, DeviceUuid, FilesystemType,
    FilesystemVersion, MountPoint, SerialNumber,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct BlockDevicesCollector {
    name: FacetName,
    identity: CollectorIdentity,
    lsblk: Option<Lsblk>,
}

impl BlockDevicesCollector {
    pub fn new() -> Self {
        Self::reading(Lsblk::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(lsblk: Option<Lsblk>) -> Self {
        Self {
            name: FacetName::new("block_devices").expect("`block_devices` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("block_devices").expect("a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            lsblk,
        }
    }
}

impl Default for BlockDevicesCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for BlockDevicesCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `undetermined` without `lsblk`, on the same reasoning as the sockets and network
    /// collectors: a box with no `lsblk` has not stopped having disks, so `absent` would be
    /// a confident lie about the box's storage.
    fn presence(&self) -> Presence {
        match self.lsblk {
            Some(_) => Presence::Present,
            None => Presence::Undetermined {
                reason: "`lsblk` was not found, so this host's block devices cannot be told"
                    .to_owned(),
            },
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let lsblk = self
            .lsblk
            .as_ref()
            .ok_or_else(|| CollectionError::new("`lsblk` was not found"))?;

        Ok(Observation::from(&lsblk.read()?))
    }
}
