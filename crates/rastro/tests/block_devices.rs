//! Reading the host's storage, without needing an `lsblk` to run.
//!
//! The fixture is the real output of `lsblk -J -b -l -o ...` on the development box, with a
//! device-mapper volume and an unmounted partition added to cover the stacking and
//! empty-filesystem cases the box itself does not have.

use rastro::collectors::block_devices::{BlockDevicesCollector, DeviceName, DeviceTree, Lsblk};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar};

const LISTING: &str = r#"{
  "blockdevices": [
    {"name":"sda","kname":"sda","pkname":null,"type":"disk","size":107374182400,
     "fstype":null,"fsver":null,"uuid":null,"label":null,"partuuid":null,"partlabel":null,
     "mountpoints":[null],"ro":false,"rm":false,"rota":true,"model":"HARDDISK",
     "serial":null,"log-sec":512,"phy-sec":512},
    {"name":"sda1","kname":"sda1","pkname":"sda","type":"part","size":107239947776,
     "fstype":"ext4","fsver":"1.0","uuid":"64549d8e-12e9-4d81-838a-5176f304ce1a",
     "label":null,"partuuid":"a1dba2a1-9f17-4a34-877b-ee83bca9c7d7","partlabel":null,
     "mountpoints":["/"],"ro":false,"rm":false,"rota":true,"model":null,"serial":null,
     "log-sec":512,"phy-sec":512},
    {"name":"sda15","kname":"sda15","pkname":"sda","type":"part","size":133169152,
     "fstype":"vfat","fsver":"FAT16","uuid":"AB66-741E","label":null,
     "partuuid":"30efd87a-f5b9-41d0-a3bf-bd40ba04abe8","partlabel":null,
     "mountpoints":["/boot/efi"],"ro":false,"rm":false,"rota":true,"model":null,
     "serial":null,"log-sec":512,"phy-sec":512},
    {"name":"sdb1","kname":"sdb1","pkname":"sdb","type":"part","size":1048576,
     "fstype":null,"fsver":null,"uuid":null,"label":null,"partuuid":null,"partlabel":null,
     "mountpoints":[null],"ro":true,"rm":true,"rota":false,"model":null,"serial":"S123",
     "log-sec":512,"phy-sec":4096},
    {"name":"dm-0","kname":"dm-0","pkname":"sda1","type":"lvm","size":53687091200,
     "fstype":"xfs","fsver":null,"uuid":"aaaabbbb-cccc-dddd-eeee-ffff00001111",
     "label":"data","partuuid":null,"partlabel":null,
     "mountpoints":["/srv","/var/lib/thing"],"ro":false,"rm":false,"rota":true,
     "model":null,"serial":null,"log-sec":512,"phy-sec":512}
  ]
}"#;

fn tree() -> DeviceTree {
    Lsblk::parse(LISTING).expect("this listing is well formed")
}

fn device(tree: &DeviceTree, name: &str) -> rastro::collectors::block_devices::BlockDevice {
    tree.devices()
        .get(&DeviceName::new(name).expect("a legal device name"))
        .unwrap_or_else(|| panic!("expected a device named {name}"))
        .clone()
}

fn object_of(observation: &Observation) -> Vec<(String, Observation)> {
    match observation.content() {
        Content::Object(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn field(observation: &Observation, name: &str) -> Observation {
    object_of(observation)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("expected a {name:?} field"))
}

fn keys_of(observation: &Observation) -> Vec<String> {
    object_of(observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

#[test]
fn parse_reads_a_whole_disk() {
    // Act
    let sda = device(&tree(), "sda");

    // Assert: a disk has a model and no filesystem.
    assert_eq!(sda.parent, None);
    assert_eq!(sda.device_type.as_str(), "disk");
    assert_eq!(sda.size.bytes(), 107374182400);
    assert_eq!(sda.filesystem_type, None);
    assert_eq!(
        sda.model.as_ref().map(|model| model.as_str()),
        Some("HARDDISK")
    );
}

#[test]
fn parse_records_the_size_in_bytes_rather_than_a_rounded_rendering() {
    // Act: `lsblk` without `-b` reports `100G` and `99.9G`, and two partitions differing by
    // a megabyte both round to `99.9G`.
    let tree = tree();

    // Assert
    assert_eq!(device(&tree, "sda").size.bytes(), 107374182400);
    assert_eq!(device(&tree, "sda1").size.bytes(), 107239947776);
}

#[test]
fn parse_keeps_the_tree_as_a_parent_link() {
    // Act: `lsblk` nests, and asked for a flat list it reports `pkname` instead, which says
    // the same thing losslessly.
    let tree = tree();

    // Assert
    assert_eq!(
        device(&tree, "sda1")
            .parent
            .as_ref()
            .map(|name| name.as_str()),
        Some("sda")
    );
    assert_eq!(
        device(&tree, "dm-0")
            .parent
            .as_ref()
            .map(|name| name.as_str()),
        Some("sda1")
    );
}

#[test]
fn parse_reads_a_device_stacked_on_a_partition() {
    // Act
    let volume = device(&tree(), "dm-0");

    // Assert
    assert_eq!(volume.device_type.as_str(), "lvm");
    assert_eq!(
        volume.filesystem_type.as_ref().map(|kind| kind.as_str()),
        Some("xfs")
    );
    assert_eq!(
        volume.filesystem_label.as_ref().map(|label| label.as_str()),
        Some("data")
    );
}

#[test]
fn parse_reads_a_null_mountpoint_list_as_no_mount_points() {
    // Act: `lsblk` writes `[null]` for a device mounted nowhere, which is the case that
    // catches a naive reader.
    let sda = device(&tree(), "sda");

    // Assert
    assert!(sda.mount_points.is_empty());
}

#[test]
fn parse_reads_a_device_mounted_in_more_than_one_place() {
    // Act: a bind mount, or a btrfs subvolume mounted twice, gives one device two mount
    // points.
    let volume = device(&tree(), "dm-0");

    // Assert: sorted, so the order `lsblk` used never reaches the document.
    assert_eq!(
        volume
            .mount_points
            .iter()
            .map(|point| point.as_str())
            .collect::<Vec<&str>>(),
        ["/srv", "/var/lib/thing"]
    );
}

#[test]
fn parse_tells_a_filesystem_uuid_apart_from_a_partition_uuid() {
    // Act: reformatting a partition changes the first and not the second, which is how a
    // reformat shows up in a facet where the name and the size did not move.
    let partition = device(&tree(), "sda1");

    // Assert
    assert_eq!(
        partition.filesystem_uuid.as_ref().map(|uuid| uuid.as_str()),
        Some("64549d8e-12e9-4d81-838a-5176f304ce1a")
    );
    assert_eq!(
        partition.partition_uuid.as_ref().map(|uuid| uuid.as_str()),
        Some("a1dba2a1-9f17-4a34-877b-ee83bca9c7d7")
    );
}

#[test]
fn parse_keeps_a_volume_identifier_that_is_not_a_uuid() {
    // Act: a vfat filesystem's identifier is a 32-bit volume id, which is why the type is
    // text rather than a parsed UUID.
    let efi = device(&tree(), "sda15");

    // Assert
    assert_eq!(
        efi.filesystem_uuid.as_ref().map(|uuid| uuid.as_str()),
        Some("AB66-741E")
    );
    assert_eq!(
        efi.filesystem_version
            .as_ref()
            .map(|version| version.as_str()),
        Some("FAT16")
    );
}

#[test]
fn parse_reads_an_empty_partition_as_having_no_filesystem() {
    // Act
    let empty = device(&tree(), "sdb1");

    // Assert
    assert_eq!(empty.filesystem_type, None);
    assert_eq!(empty.filesystem_uuid, None);
    assert!(empty.mount_points.is_empty());
}

#[test]
fn parse_reads_the_boolean_columns() {
    // Act
    let tree = tree();

    // Assert
    let removable = device(&tree, "sdb1");
    assert!(removable.read_only);
    assert!(removable.removable);
    assert!(!removable.rotational);
    assert_eq!(removable.physical_sector_size.bytes(), 4096);
    assert_eq!(removable.logical_sector_size.bytes(), 512);
}

#[test]
fn parse_keys_devices_by_name() {
    // Act
    let observation = Observation::from(&tree());

    // Assert: sorted, so the order `lsblk` walked sysfs in never reaches the document.
    assert_eq!(
        keys_of(&observation),
        ["dm-0", "sda", "sda1", "sda15", "sdb1"]
    );
}

#[test]
fn parse_refuses_a_device_reported_twice() {
    // Arrange: the kernel enforces one device per name, so a repeat means rastro misread.
    let repeated = r#"{"blockdevices":[
      {"name":"sda","type":"disk","size":1,"ro":false,"rm":false,"rota":true,
       "log-sec":512,"phy-sec":512},
      {"name":"sda","type":"disk","size":2,"ro":false,"rm":false,"rota":true,
       "log-sec":512,"phy-sec":512}]}"#;

    // Act & Assert
    assert!(Lsblk::parse(repeated).is_err());
}

#[test]
fn parse_refuses_output_that_is_not_json() {
    // Act
    let result = Lsblk::parse("NAME TYPE SIZE\nsda disk 100G\n");

    // Assert
    let failure = result.expect_err("the table form is not JSON");
    assert!(
        failure.to_string().contains("lsblk"),
        "the message must name the tool, got: {failure}"
    );
}

#[test]
fn parse_accepts_a_box_with_no_block_devices() {
    // Act: a container with no devices of its own really does report none.
    let tree = Lsblk::parse(r#"{"blockdevices":[]}"#).expect("an empty listing is well formed");

    // Assert
    assert!(tree.is_empty());
}

#[test]
fn a_device_renders_a_null_for_every_column_it_has_no_value_for() {
    // Act
    let observation = Observation::from(&tree());
    let sda = field(&observation, "sda");

    // Assert: a consumer never meets a key that is sometimes absent.
    assert_eq!(
        field(&sda, "filesystem_type").content(),
        &Content::Scalar(Scalar::Null)
    );
    assert_eq!(
        field(&sda, "parent").content(),
        &Content::Scalar(Scalar::Null)
    );
    assert_eq!(
        field(&sda, "size").content(),
        &Content::Scalar(Scalar::Integer(107374182400))
    );
}

#[test]
fn every_device_renders_the_same_keys() {
    // Act
    let observation = Observation::from(&tree());

    // Assert
    let expected = keys_of(&field(&observation, "sda"));
    for (name, device) in object_of(&observation) {
        assert_eq!(keys_of(&device), expected, "device {name} differs");
    }
}

#[test]
fn presence_is_undetermined_without_lsblk_rather_than_absent() {
    // Act: a box with no `lsblk` has not stopped having disks.
    match BlockDevicesCollector::reading(None).presence() {
        Presence::Undetermined { reason } => assert!(
            reason.contains("cannot be told"),
            "the reason must say rastro could not see, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn presence_is_present_when_lsblk_is_on_the_host() {
    // Arrange
    let lsblk = Lsblk::using(
        rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
            .expect("every unix has /bin/sh"),
    );

    // Act & Assert
    assert_eq!(
        BlockDevicesCollector::reading(Some(lsblk)).presence(),
        Presence::Present
    );
}

#[test]
fn collect_fails_rather_than_reporting_no_storage_without_lsblk() {
    // Act & Assert
    assert!(BlockDevicesCollector::reading(None).collect().is_err());
}
