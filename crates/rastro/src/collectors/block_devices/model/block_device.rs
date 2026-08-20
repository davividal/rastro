//! One block device.

use rastro_collector::{ByteSize, Observation};

use crate::collectors::block_devices::value_objects::{
    DeviceLabel, DeviceModel, DeviceName, DeviceType, DeviceUuid, FilesystemType,
    FilesystemVersion, MountPoint, SerialNumber,
};

/// A block device as rastro means it.
///
/// The device's name is not a field here, because it is the key this is filed under.
///
/// **The tree is kept as a parent link rather than as nesting**, and that is a deliberate
/// departure from the shape `lsblk` hands over. `lsblk` nests partitions inside their disk,
/// and LVM volumes inside those; asked for a flat list it reports `pkname` instead, which
/// says the same thing losslessly. Flat and keyed by name is what gives this facet the diff
/// granularity every other keyed facet here has: adding a partition is one new key rather
/// than a changed list three levels down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDevice {
    /// The device this one sits on, absent for a whole device.
    pub parent: Option<DeviceName>,
    pub device_type: DeviceType,
    pub size: ByteSize,
    /// Absent when the device holds no filesystem rastro's source could identify, which
    /// includes an empty partition and a whole disk carrying a partition table.
    pub filesystem_type: Option<FilesystemType>,
    pub filesystem_version: Option<FilesystemVersion>,
    /// The filesystem's own identifier.
    pub filesystem_uuid: Option<DeviceUuid>,
    pub filesystem_label: Option<DeviceLabel>,
    /// The partition table entry's identifier, which is a different thing from the
    /// filesystem's: reformatting a partition changes the second and not the first.
    pub partition_uuid: Option<DeviceUuid>,
    pub partition_label: Option<DeviceLabel>,
    /// Sorted, and empty for a device mounted nowhere.
    pub mount_points: Vec<MountPoint>,
    pub read_only: bool,
    pub removable: bool,
    pub rotational: bool,
    pub model: Option<DeviceModel>,
    pub serial_number: Option<SerialNumber>,
    pub logical_sector_size: ByteSize,
    pub physical_sector_size: ByteSize,
}

impl From<&BlockDevice> for Observation {
    fn from(device: &BlockDevice) -> Self {
        Observation::object([
            ("device_type", Observation::from(&device.device_type)),
            (
                "filesystem_label",
                text(device.filesystem_label.as_ref().map(DeviceLabel::as_str)),
            ),
            (
                "filesystem_type",
                text(device.filesystem_type.as_ref().map(FilesystemType::as_str)),
            ),
            (
                "filesystem_uuid",
                text(device.filesystem_uuid.as_ref().map(DeviceUuid::as_str)),
            ),
            (
                "filesystem_version",
                text(
                    device
                        .filesystem_version
                        .as_ref()
                        .map(FilesystemVersion::as_str),
                ),
            ),
            (
                "logical_sector_size",
                Observation::integer(device.logical_sector_size.bytes()),
            ),
            (
                "model",
                text(device.model.as_ref().map(DeviceModel::as_str)),
            ),
            (
                "mount_points",
                Observation::list(device.mount_points.iter().map(Observation::from)),
            ),
            (
                "parent",
                text(device.parent.as_ref().map(DeviceName::as_str)),
            ),
            (
                "partition_label",
                text(device.partition_label.as_ref().map(DeviceLabel::as_str)),
            ),
            (
                "partition_uuid",
                text(device.partition_uuid.as_ref().map(DeviceUuid::as_str)),
            ),
            (
                "physical_sector_size",
                Observation::integer(device.physical_sector_size.bytes()),
            ),
            ("read_only", Observation::boolean(device.read_only)),
            ("removable", Observation::boolean(device.removable)),
            ("rotational", Observation::boolean(device.rotational)),
            (
                "serial_number",
                text(device.serial_number.as_ref().map(SerialNumber::as_str)),
            ),
            ("size", Observation::integer(device.size.bytes())),
        ])
    }
}

fn text(value: Option<&str>) -> Observation {
    value.map_or_else(Observation::null, Observation::text)
}
