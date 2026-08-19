//! Reading `/proc/mounts`, without needing a `/proc` to read it from.

use rastro::collectors::mounts::{
    Device, FilesystemType, Mount, MountOption, MountPoint, MountTable, MountsCollector, ProcMounts,
};
use rastro_collector::{Collector, Presence};
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
    // Arrange: four of the sequences the kernel emits.
    let table = parsed("dev /a\\011b\\012c\\134d ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/a\tb\nc\\d");
}

#[test]
fn parse_leaves_a_backslash_alone_when_it_is_not_an_escape() {
    // Arrange: a backslash only escapes when three octal digits follow it, so neither of
    // these is one.
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

/// A fixture on disk, named per test so parallel runs cannot clash.
fn fixture(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("rastro-mounts-{name}"));
    std::fs::write(&path, contents).expect("the temp directory should be writable");
    path
}

#[test]
fn read_translates_a_table_on_disk_into_the_model() {
    // Arrange: every other test goes through `parse`, which leaves the file reading and
    // its error path uncovered.
    let path = fixture("read", TWO_MOUNTS);
    let source = ProcMounts::at(&path);

    // Act
    let table = source.read().expect("this fixture is well formed");

    // Assert
    assert_eq!(table.mounts().len(), 2);
}

#[test]
fn read_names_the_file_it_could_not_open() {
    // Arrange
    let path = std::env::temp_dir().join("rastro-mounts-absent");
    let source = ProcMounts::at(&path);

    // Act
    let failure = source.read().expect_err("a missing file is a failure");

    // Assert: an operator reading stderr should not have to guess which file it was.
    assert!(
        failure.to_string().contains(&path.display().to_string()),
        "got {failure}"
    );
}

#[test]
fn a_table_that_cannot_be_read_becomes_a_failed_collection() {
    // Arrange: an unreadable table is a failure to read the mounts, never evidence that
    // the host has none, so it must surface from `collect` rather than from `presence`.
    let collector = MountsCollector::reading(ProcMounts::at(
        std::env::temp_dir().join("rastro-mounts-absent-collect"),
    ));

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Present);
    assert!(collector.collect().is_err());
}

#[test]
fn parse_keeps_one_entry_when_an_option_is_repeated() {
    // Arrange: options are a set, so the type collapses a repeat. The kernel emits none,
    // but the behaviour changed when `MountOptions` became a `BTreeSet` and was untested.
    let table = parsed("dev /data ext4 rw,relatime,rw 0 0\n");

    // Assert
    assert_eq!(options_of(first(&table)), ["relatime", "rw"]);
}

#[test]
fn parse_keeps_a_mount_point_containing_unicode_whitespace() {
    // Arrange: the kernel escapes exactly space, tab, newline and backslash, so every other
    // whitespace character arrives unescaped inside the value. U+00A0 reaches a mount point
    // from a Windows share name via CIFS. Splitting on Unicode whitespace found a seventh
    // column here and lost the entire table to one oddly named directory.
    let table = parsed("/dev/sda1 /mnt/My\u{a0}Drive ext4 rw,relatime 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/mnt/My\u{a0}Drive");
}

#[test]
fn parse_keeps_a_carriage_return_inside_a_column() {
    // Arrange: a legal byte in a filename, and not one the kernel escapes.
    let table = parsed("/dev/sda1 /mnt/od\rd ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/mnt/od\rd");
}

#[test]
fn parse_keeps_unicode_whitespace_in_the_device_and_the_ideographic_space() {
    // Arrange: the other two forms the kernel leaves alone.
    let by_device = parsed("/dev/loop0\u{a0}x /mnt ext4 rw 0 0\n");
    let ideographic = parsed("/dev/sda1 /mnt/\u{3000}share ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&by_device).device.as_str(), "/dev/loop0\u{a0}x");
    assert_eq!(
        first(&ideographic).mount_point.as_str(),
        "/mnt/\u{3000}share"
    );
}

#[test]
fn parse_decodes_an_escaped_hash_in_the_device_and_the_filesystem_type() {
    // Arrange: the device name and the filesystem type go through the kernel's `mangle`, which
    // escapes a fifth character the mount point's own escaping does not: `#`, because
    // `/etc/mtab` treats it as a comment. Missing it left a raw `\043` in the value.
    let table = parsed("/dev/disk/by-label/a\\043b /mnt fuse.od\\043d rw 0 0\n");

    // Assert
    let mount = first(&table);
    assert_eq!(mount.device.as_str(), "/dev/disk/by-label/a#b");
    assert_eq!(mount.filesystem.as_str(), "fuse.od#d");
}

#[test]
fn parse_leaves_a_hash_alone_when_the_kernel_did_not_escape_it() {
    // Arrange: the mount point is escaped by `seq_path_root`, which does not escape `#`, so a
    // literal one arrives raw and must survive as itself.
    let table = parsed("/dev/sda1 /mnt/a#b ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/mnt/a#b");
}

#[test]
fn parse_keeps_every_option_when_a_value_contains_a_stray_quote() {
    // Arrange: a real line from a 6.x kernel. A quote is a legal path character and
    // `seq_show_option` does not escape it, so an overlay whose lower directory contains one
    // used to desynchronise the quote tracking and fuse every later option into one value:
    // seven options became four, and `upperdir`, `workdir` and `uuid` ceased to exist.
    let table = parsed(concat!(
        "overlay /merged overlay ",
        "rw,seclabel,relatime,lowerdir=/base/a\"b/lower,upperdir=/base/up,",
        "workdir=/base/work,uuid=on",
        " 0 0\n"
    ));

    // Assert
    assert_eq!(
        options_of(first(&table)),
        [
            "lowerdir=/base/a\"b/lower",
            "relatime",
            "rw",
            "seclabel",
            "upperdir=/base/up",
            "uuid=on",
            "workdir=/base/work"
        ]
    );
}

#[test]
fn parse_decodes_an_escaped_comma_in_an_option_value() {
    // Arrange: `seq_show_option` escapes a comma as `\054`, so a btrfs subvolume whose name
    // contains one arrives escaped. Leaving it literal half-decoded the value, since a space in
    // the same value was already being decoded.
    let table = parsed("/dev/sda1 /mnt btrfs rw,subvol=/a\\054b 0 0\n");

    // Assert
    assert_eq!(options_of(first(&table)), ["rw", "subvol=/a,b"]);
}

#[test]
fn parse_decodes_an_escaped_equals_in_an_option_name() {
    // Arrange: an option *name* is escaped with a wider set than its value, including `=`.
    let table = parsed("/dev/sda1 /mnt ext4 rw,od\\075d=1 0 0\n");

    // Assert
    assert_eq!(options_of(first(&table)), ["od=d=1", "rw"]);
}

#[test]
fn parse_leaves_an_octal_escape_the_kernel_does_not_use_alone() {
    // Arrange: values at or above 0o200 are single bytes of a UTF-8 sequence rather than
    // characters, so reassembling them is not attempted and they stay as they are.
    let table = parsed("/dev/sda1 /mnt/a\\303b ext4 rw 0 0\n");

    // Assert
    assert_eq!(first(&table).mount_point.as_str(), "/mnt/a\\303b");
}
