//! Every block device the host has.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::block_device::BlockDevice;
use crate::collectors::block_devices::value_objects::DeviceName;

/// The block devices, keyed by name.
///
/// Keyed rather than listed, because the kernel enforces one device per name, so keying
/// loses nothing and removes the order `lsblk` walked sysfs in.
///
/// Named a tree because the parent links make one, even though the storage is flat. What is
/// *not* checked is that the links form a well-shaped tree: a device naming a parent that is
/// not in the map would be a fault, and rastro records what `lsblk` said rather than
/// auditing it, because the shape of a stacking arrangement is the kernel's business.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceTree(BTreeMap<DeviceName, BlockDevice>);

impl DeviceTree {
    pub fn new(
        devices: impl IntoIterator<Item = (DeviceName, BlockDevice)>,
    ) -> Result<Self, CollectionError> {
        let mut tree = BTreeMap::new();

        for (name, device) in devices {
            if tree.insert(name.clone(), device).is_some() {
                return Err(CollectionError::new(format!(
                    "the block device {:?} was reported twice, so the output was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(tree))
    }

    pub fn devices(&self) -> &BTreeMap<DeviceName, BlockDevice> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&DeviceTree> for Observation {
    fn from(tree: &DeviceTree) -> Self {
        Observation::object(
            tree.devices()
                .iter()
                .map(|(name, device)| (name.as_str(), Observation::from(device))),
        )
    }
}
