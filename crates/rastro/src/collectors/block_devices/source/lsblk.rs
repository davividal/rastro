//! The `lsblk` interface.

use serde::Deserialize;

use rastro_collector::{ByteSize, CollectionError};

use crate::collectors::block_devices::model::{BlockDevice, DeviceTree};
use crate::collectors::block_devices::value_objects::{
    DeviceLabel, DeviceModel, DeviceName, DeviceType, DeviceUuid, FilesystemType,
    FilesystemVersion, MountPoint, SerialNumber,
};
use crate::collectors::canonical_tool::CanonicalTool;

const PROGRAM: &str = "lsblk";

/// Ask for JSON, so the shape is rastro's rather than `lsblk`'s to format.
const JSON: &str = "-J";

/// **Sizes in bytes rather than the human-readable form `lsblk` prefers.**
///
/// Without this, `lsblk` reports `100G` and `99.9G`. That is lossy — two partitions
/// differing by a megabyte both round to `99.9G` — and it is a rendering rather than a
/// measurement, so it could change with a util-linux release while the disk did not. Bytes
/// are exact and go straight into a [`ByteSize`].
const BYTES: &str = "-b";

/// Flat output rather than the nested tree.
///
/// Paired with asking for `PKNAME`, this loses nothing: the parent link says what the
/// nesting said. It is what lets the model be keyed by name like every other keyed facet
/// here, instead of a recursive structure whose diff buries a new partition three levels
/// down.
const FLAT: &str = "-l";

/// The columns rastro asks for, in place of `lsblk`'s sparse default set.
///
/// Chosen rather than taking `-O`, which reports everything. Everything includes
/// `FSAVAIL`, `FSUSED` and `FSUSE%`, which are how full a filesystem is: those change
/// constantly on a working box and would be pure noise in a fingerprint meant for diffing.
const COLUMNS: &str = concat!(
    "NAME,KNAME,PKNAME,TYPE,SIZE,FSTYPE,FSVER,UUID,LABEL,PARTUUID,PARTLABEL,",
    "MOUNTPOINTS,RO,RM,ROTA,MODEL,SERIAL,LOG-SEC,PHY-SEC"
);

/// `lsblk`'s spelling of a block device, kept apart from rastro's meaning.
///
/// Almost every field is nullable, which is the tool's shape rather than defensiveness: a
/// whole disk has a model and no filesystem, a partition has a filesystem and no model, and
/// an empty partition has neither.
#[derive(Debug, Clone, Deserialize)]
struct DeviceObject {
    name: String,
    #[serde(default)]
    pkname: Option<String>,
    #[serde(rename = "type")]
    device_type: String,
    size: u64,
    #[serde(default)]
    fstype: Option<String>,
    #[serde(default)]
    fsver: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    partuuid: Option<String>,
    #[serde(default)]
    partlabel: Option<String>,
    /// A list that legitimately contains `null` for a device mounted nowhere, which is
    /// `lsblk`'s way of writing an empty list.
    #[serde(default)]
    mountpoints: Vec<Option<String>>,
    ro: bool,
    rm: bool,
    rota: bool,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial: Option<String>,
    #[serde(rename = "log-sec")]
    logical_sector_size: u64,
    #[serde(rename = "phy-sec")]
    physical_sector_size: u64,
}

/// The document `lsblk -J` wraps its devices in.
#[derive(Debug, Clone, Deserialize)]
struct Listing {
    blockdevices: Vec<DeviceObject>,
}

/// The host's block devices, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lsblk {
    tool: CanonicalTool,
}

impl Lsblk {
    /// Finds `lsblk`, or reports that this host does not have it.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located, so the argument vector above is reachable
    /// from a test rather than being the one part of the exec route nothing can observe.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    pub fn read(&self) -> Result<DeviceTree, CollectionError> {
        Self::parse(&self.tool.run(&[JSON, BYTES, FLAT, "-o", COLUMNS])?)
    }

    /// Translates the tool's output into the model.
    ///
    /// Separate from [`Self::read`] so the whole translation is exercised from a fixture,
    /// with no `lsblk` to run.
    pub fn parse(output: &str) -> Result<DeviceTree, CollectionError> {
        let listing: Listing = serde_json::from_str(output).map_err(|error| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM}` reported as JSON: {error}"
            ))
        })?;

        DeviceTree::new(
            listing
                .blockdevices
                .iter()
                .map(DeviceObject::to_device)
                .collect::<Result<Vec<_>, CollectionError>>()?,
        )
    }
}

impl DeviceObject {
    fn to_device(&self) -> Result<(DeviceName, BlockDevice), CollectionError> {
        let mut mount_points = self
            .mountpoints
            .iter()
            .flatten()
            .map(MountPoint::new)
            .collect::<Result<Vec<MountPoint>, CollectionError>>()?;
        mount_points.sort();

        let device = BlockDevice {
            parent: present(self.pkname.as_deref())
                .map(DeviceName::new)
                .transpose()?,
            device_type: DeviceType::new(&self.device_type)?,
            size: ByteSize::new(self.size, "device size")?,
            filesystem_type: present(self.fstype.as_deref())
                .map(FilesystemType::new)
                .transpose()?,
            filesystem_version: present(self.fsver.as_deref())
                .map(FilesystemVersion::new)
                .transpose()?,
            filesystem_uuid: present(self.uuid.as_deref())
                .map(DeviceUuid::new)
                .transpose()?,
            filesystem_label: present(self.label.as_deref())
                .map(DeviceLabel::new)
                .transpose()?,
            partition_uuid: present(self.partuuid.as_deref())
                .map(DeviceUuid::new)
                .transpose()?,
            partition_label: present(self.partlabel.as_deref())
                .map(DeviceLabel::new)
                .transpose()?,
            mount_points,
            read_only: self.ro,
            removable: self.rm,
            rotational: self.rota,
            model: present(self.model.as_deref())
                .map(DeviceModel::new)
                .transpose()?,
            serial_number: present(self.serial.as_deref())
                .map(SerialNumber::new)
                .transpose()?,
            logical_sector_size: ByteSize::new(self.logical_sector_size, "sector size")?,
            physical_sector_size: ByteSize::new(self.physical_sector_size, "sector size")?,
        };

        Ok((DeviceName::new(&self.name)?, device))
    }
}

/// A column's value, if it has one worth building a type around.
///
/// **An empty string is folded into absence alongside `null`.** `lsblk` writes `null` when
/// asked for JSON, but the same column is blank in its table output, and neither is a value.
/// Folding them here is what lets every optional column above be one line.
fn present(column: Option<&str>) -> Option<&str> {
    column.filter(|value| !value.is_empty())
}
