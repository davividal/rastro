//! What is mounted where, and how.

use rastro_collector::Observation;

use crate::collectors::mounts::value::{Device, FilesystemType, MountOptions, MountPoint};

/// One mount, in rastro's terms rather than any one host interface's.
///
/// A plain aggregate: every field is already a validated value, so there is no
/// invariant left for this type to hold. It carries four fields because those are
/// the ones that describe state, not because a particular file has four columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub device: Device,
    pub mount_point: MountPoint,
    pub filesystem: FilesystemType,
    pub options: MountOptions,
}

impl From<&Mount> for Observation {
    fn from(mount: &Mount) -> Self {
        Observation::object([
            ("device", Observation::text(mount.device.as_str())),
            ("mount_point", Observation::text(mount.mount_point.as_str())),
            ("filesystem", Observation::text(mount.filesystem.as_str())),
            ("options", Observation::from(&mount.options)),
        ])
    }
}
