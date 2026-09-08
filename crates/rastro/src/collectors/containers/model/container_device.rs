//! A device passed into a container.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

/// One host device a container was given, and what it may do with it.
///
/// **The sharpest thing a container can be handed short of privilege.** `--device
/// /dev/sda:/dev/sda:rwm` gives a container the block device the host boots from, and
/// nothing else in a fingerprint would say so: the mount table does not show it, and the
/// container looks ordinary from the outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDevice {
    pub host_path: AbsolutePath,
    /// cgroup's own three letters, read, write and mknod, as `rwm`.
    pub permissions: NonEmptyText,
}

impl From<&ContainerDevice> for Observation {
    fn from(device: &ContainerDevice) -> Self {
        Observation::object([
            ("host_path", Observation::text(device.host_path.as_str())),
            (
                "permissions",
                Observation::text(device.permissions.as_str()),
            ),
        ])
    }
}
