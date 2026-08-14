//! Parsing the mount table, without needing a `/proc` to read it from.

use rastro::collectors::parse_mount_table;
use rastro_fingerprint::{Content, Observation, Scalar};

const TWO_MOUNTS: &str = "\
/dev/sda1 / ext4 rw,relatime,errors=remount-ro 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
";

fn mounts_of(observation: &Observation) -> &[Observation] {
    match observation.content() {
        Content::List(items) => items,
        other => panic!("expected a list of mounts, got {other:?}"),
    }
}

fn field(mount: &Observation, key: &str) -> String {
    let Content::Object(entries) = mount.content() else {
        panic!("expected a mount object");
    };
    match entries[key].content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected {key} to be text, got {other:?}"),
    }
}

fn options_of(mount: &Observation) -> Vec<String> {
    let Content::Object(entries) = mount.content() else {
        panic!("expected a mount object");
    };
    let Content::List(options) = entries["options"].content() else {
        panic!("expected options to be a list");
    };
    options
        .iter()
        .map(|option| match option.content() {
            Content::Scalar(Scalar::Text(value)) => value.clone(),
            other => panic!("expected an option to be text, got {other:?}"),
        })
        .collect()
}

#[test]
fn parse_mount_table_reads_the_four_fields_that_matter() {
    // Act
    let observation = parse_mount_table(TWO_MOUNTS).expect("this table is well formed");

    // Assert
    let first = &mounts_of(&observation)[0];
    assert_eq!(field(first, "device"), "/dev/sda1");
    assert_eq!(field(first, "mount_point"), "/");
    assert_eq!(field(first, "filesystem"), "ext4");
}

#[test]
fn parse_mount_table_sorts_the_options() {
    // Act
    let observation = parse_mount_table(TWO_MOUNTS).expect("this table is well formed");

    // Assert: the kernel's order for a set of flags is arbitrary churn.
    assert_eq!(
        options_of(&mounts_of(&observation)[0]),
        ["errors=remount-ro", "relatime", "rw"]
    );
}

#[test]
fn parse_mount_table_keeps_the_kernel_order_of_mounts() {
    // Act
    let observation = parse_mount_table(TWO_MOUNTS).expect("this table is well formed");

    // Assert: that order is stable between runs and carries mount stacking,
    // which sorting would discard.
    let mounts = mounts_of(&observation);
    assert_eq!(field(&mounts[0], "mount_point"), "/");
    assert_eq!(field(&mounts[1], "mount_point"), "/proc");
}

#[test]
fn parse_mount_table_keeps_both_entries_when_a_mount_point_repeats() {
    // Arrange: stacked and bind mounts legitimately share a mount point.
    let stacked = "\
/dev/sdb1 /data ext4 rw 0 0
/dev/sdc1 /data ext4 ro 0 0
";

    // Act
    let observation = parse_mount_table(stacked).expect("this table is well formed");

    // Assert: keying by mount point would have dropped one of these silently.
    let mounts = mounts_of(&observation);
    assert_eq!(mounts.len(), 2);
    assert_eq!(field(&mounts[0], "device"), "/dev/sdb1");
    assert_eq!(field(&mounts[1], "device"), "/dev/sdc1");
}

#[test]
fn parse_mount_table_ignores_blank_lines() {
    // Act
    let observation =
        parse_mount_table("\nproc /proc proc rw 0 0\n\n").expect("blank lines are not an error");

    // Assert
    assert_eq!(mounts_of(&observation).len(), 1);
}

#[test]
fn parse_mount_table_refuses_a_line_it_cannot_read() {
    // Act
    let result = parse_mount_table("proc /proc\n");

    // Assert: a table rastro cannot parse is reported, never quietly skipped.
    assert!(result.is_err());
}

#[test]
fn parse_mount_table_keeps_a_quoted_option_value_whole() {
    // Arrange: SELinux contexts carry commas inside quotes.
    let table = concat!(
        "overlay / overlay ",
        "rw,context=\"system_u:object_r:container_file_t:s0:c132,c369\",relatime",
        " 0 0\n"
    );

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert: splitting on every comma would invent two bogus options here.
    assert_eq!(
        options_of(&mounts_of(&observation)[0]),
        [
            "context=\"system_u:object_r:container_file_t:s0:c132,c369\"",
            "relatime",
            "rw"
        ]
    );
}

#[test]
fn parse_mount_table_decodes_an_escaped_space_in_a_mount_point() {
    // Arrange: the kernel escapes whitespace so that the table stays safe to
    // tokenise. The escape is a transport encoding, not the state of the host.
    let table = "/dev/sdb1 /mnt/My\\040Drive ext4 rw 0 0\n";

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert
    assert_eq!(
        field(&mounts_of(&observation)[0], "mount_point"),
        "/mnt/My Drive"
    );
}

#[test]
fn parse_mount_table_decodes_escapes_in_the_device_too() {
    // Arrange
    let table = "/dev/disk/by-label/My\\040Disk /data ext4 rw 0 0\n";

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert
    assert_eq!(
        field(&mounts_of(&observation)[0], "device"),
        "/dev/disk/by-label/My Disk"
    );
}

#[test]
fn parse_mount_table_decodes_tab_newline_and_backslash() {
    // Arrange: the four sequences the kernel actually emits.
    let table = "dev /a\\011b\\012c\\134d ext4 rw 0 0\n";

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert
    assert_eq!(
        field(&mounts_of(&observation)[0], "mount_point"),
        "/a\tb\nc\\d"
    );
}

#[test]
fn parse_mount_table_leaves_a_backslash_alone_when_it_is_not_an_escape() {
    // Arrange: only the four sequences the kernel writes are escapes, so `\061`
    // is three octal digits and is deliberately left alone too.
    let table = "dev /a\\b\\9 ext4 rw 0 0\n";

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert
    assert_eq!(
        field(&mounts_of(&observation)[0], "mount_point"),
        "/a\\b\\9"
    );
}

#[test]
fn parse_mount_table_decodes_an_escaped_option_before_sorting_it() {
    // Arrange: `a\040b` sorts *after* `aB` while escaped, because `\` is 0x5C
    // and `B` is 0x42, but *before* it once decoded, because a space is 0x20.
    // Sorting the transport encoding would order options by an artefact of it.
    let table = "dev /data ext4 aB,a\\040b 0 0\n";

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert
    assert_eq!(options_of(&mounts_of(&observation)[0]), ["a b", "aB"]);
}

#[test]
fn parse_mount_table_decodes_an_escape_in_the_filesystem_type() {
    // Arrange: the kernel mangles the fstype field too.
    let table = "dev /data od\\040d rw 0 0\n";

    // Act
    let observation = parse_mount_table(table).expect("this table is well formed");

    // Assert
    assert_eq!(field(&mounts_of(&observation)[0], "filesystem"), "od d");
}
