//! Reading `/proc/mounts`, without needing a `/proc` to read it from.

use rastro::collectors::mounts::{
    Device, FilesystemType, Mount, MountOption, MountPoint, MountTable, ProcMounts,
};
use rastro_fingerprint::{Content, Observation, Scalar};

const TWO_MOUNTS: &str = "\
/dev/sda1 / ext4 rw,relatime,errors=remount-ro 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
";

fn parsed(table: &str) -> MountTable {
    ProcMounts::parse(table).expect("this table is well formed")
}

fn first(table: &MountTable) -> &Mount {
    table.mounts().first().expect("at least one mount")
}

fn options_of(mount: &Mount) -> Vec<&str> {
    mount.options.iter().map(MountOption::as_str).collect()
}

#[test]
fn parse_reads_the_four_fields_that_matter() {
    // Act
    let table = parsed(TWO_MOUNTS);

    // Assert
    let mount = first(&table);
    assert_eq!(
        mount.device,
        Device::new("/dev/sda1").expect("a legal device")
    );
    assert_eq!(
        mount.mount_point,
        MountPoint::new("/").expect("a legal mount point")
    );
    assert_eq!(
        mount.filesystem,
        FilesystemType::new("ext4").expect("a legal filesystem type")
    );
}

#[test]
fn parse_sorts_the_options() {
    // Act
    let table = parsed(TWO_MOUNTS);

    // Assert: the kernel's order for a set of flags is arbitrary churn.
    assert_eq!(
        options_of(first(&table)),
        ["errors=remount-ro", "relatime", "rw"]
    );
}

#[test]
fn parse_keeps_the_kernel_order_of_mounts() {
    // Act
    let table = parsed(TWO_MOUNTS);

    // Assert: that order is stable between runs and carries mount stacking,
    // which sorting would discard.
    let mounts = table.mounts();
    assert_eq!(mounts[0].mount_point.as_str(), "/");
    assert_eq!(mounts[1].mount_point.as_str(), "/proc");
}

#[test]
fn parse_keeps_both_entries_when_a_mount_point_repeats() {
    // Arrange: stacked and bind mounts legitimately share a mount point.
    let stacked = "\
/dev/sdb1 /data ext4 rw 0 0
/dev/sdc1 /data ext4 ro 0 0
";

    // Act
    let table = parsed(stacked);

    // Assert: keying by mount point would have dropped one of these silently.
    let mounts = table.mounts();
    assert_eq!(mounts.len(), 2);
    assert_eq!(mounts[0].device.as_str(), "/dev/sdb1");
    assert_eq!(mounts[1].device.as_str(), "/dev/sdc1");
}

#[test]
fn parse_ignores_blank_lines() {
    // Act
    let table = parsed("\nproc /proc proc rw 0 0\n\n");

    // Assert
    assert_eq!(table.mounts().len(), 1);
}

#[test]
fn parse_refuses_a_line_it_cannot_read() {
    // Act
    let result = ProcMounts::parse("proc /proc\n");

    // Assert: a table rastro cannot parse is reported, never quietly skipped.
    assert!(result.is_err());
}

#[test]
fn parse_refuses_a_line_missing_the_trailing_columns() {
    // Act: the kernel writes six columns for every mount, so five means this is not
    // the interface it claims to be.
    let result = ProcMounts::parse("/dev/sda1 / ext4 rw 0\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_refuses_a_mount_point_that_is_not_absolute() {
    // Act: the fields are positional, so a relative mount point means the line
    // was tokenised into the wrong slots.
    let result = ProcMounts::parse("proc proc proc rw 0 0\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_keeps_a_quoted_option_value_whole() {
    // Arrange: SELinux contexts carry commas inside quotes.
    let table = concat!(
        "overlay / overlay ",
        "rw,context=\"system_u:object_r:container_file_t:s0:c132,c369\",relatime",
        " 0 0\n"
    );

    // Act
    let table = parsed(table);

    // Assert: splitting on every comma would invent two bogus options here.
    assert_eq!(
        options_of(first(&table)),
        [
            "context=\"system_u:object_r:container_file_t:s0:c132,c369\"",
            "relatime",
            "rw"
        ]
    );
}

#[test]
fn parse_decodes_an_escaped_space_in_a_mount_point() {
    // Arrange: the kernel escapes whitespace so that the table stays safe to
    // tokenise. The escape is a transport encoding, not the state of the host.
    let table = parsed("/dev/sdb1 /mnt/My\\040Drive ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/mnt/My Drive");
}

#[test]
fn parse_decodes_escapes_in_the_device_too() {
    // Act
    let table = parsed("/dev/disk/by-label/My\\040Disk /data ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).device.as_str(), "/dev/disk/by-label/My Disk");
}

#[test]
fn parse_decodes_tab_newline_and_backslash() {
    // Arrange: the four sequences the kernel actually emits.
    let table = parsed("dev /a\\011b\\012c\\134d ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/a\tb\nc\\d");
}

#[test]
fn parse_leaves_a_backslash_alone_when_it_is_not_an_escape() {
    // Arrange: only the four sequences the kernel writes are escapes, so `\061`
    // is three octal digits and is deliberately left alone too.
    let table = parsed("dev /a\\b\\9 ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/a\\b\\9");
}

#[test]
fn parse_decodes_an_escaped_option_before_sorting_it() {
    // Arrange: `a\040b` sorts *after* `aB` while escaped, because `\` is 0x5C and
    // `B` is 0x42, but *before* it once decoded, because a space is 0x20. Sorting
    // the transport encoding would order options by an artefact of it.
    let table = parsed("dev /data ext4 aB,a\\040b 0 0\n");

    // Assert
    assert_eq!(options_of(first(&table)), ["a b", "aB"]);
}

#[test]
fn parse_decodes_an_escape_in_the_filesystem_type() {
    // Arrange: the kernel mangles the fstype field too.
    let table = parsed("dev /data od\\040d rw 0 0\n");

    // Assert
    assert_eq!(first(&table).filesystem.as_str(), "od d");
}

#[test]
fn the_observation_of_a_mount_carries_the_contracted_keys() {
    // Arrange: the only test that reads the rendered shape, because the key names
    // are the output contract and a rename would otherwise pass every assertion
    // above.
    let table = parsed("/dev/sda1 / ext4 rw,relatime 0 0\n");

    // Act
    let observation = Observation::from(first(&table));

    // Assert
    let Content::Object(entries) = observation.content() else {
        panic!("a mount renders as an object");
    };
    assert_eq!(
        entries.keys().map(String::as_str).collect::<Vec<&str>>(),
        ["device", "filesystem", "mount_point", "options"]
    );
    assert_eq!(
        entries["device"].content(),
        &Content::Scalar(Scalar::Text("/dev/sda1".to_owned()))
    );
    let Content::List(options) = entries["options"].content() else {
        panic!("options render as a list");
    };
    assert_eq!(options.len(), 2);
}
