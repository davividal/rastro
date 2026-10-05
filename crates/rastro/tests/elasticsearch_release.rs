//! Which release a server jar's name says, and when it says none.

use rastro::collectors::elasticsearch::{Release, ReleaseSupport, ResidentNode, SupportedRelease};

mod support;

use support::fs_tree::{scratch_tree, write};

#[test]
fn parse_reads_a_released_version() {
    // Act
    let release = Release::parse("7.17.29");

    // Assert
    assert_eq!(
        release.map(|release| release.to_string()).as_deref(),
        Some("7.17.29")
    );
}

#[test]
fn parse_refuses_a_pre_release() {
    // Act & Assert
    assert_eq!(Release::parse("9.6.0-SNAPSHOT"), None);
}

#[test]
fn parse_refuses_a_version_missing_its_patch() {
    // Act & Assert
    assert_eq!(Release::parse("9.5"), None);
}

#[test]
fn parse_refuses_a_fourth_component() {
    // Act & Assert
    assert_eq!(Release::parse("9.5.4.1"), None);
}

#[test]
fn parse_refuses_an_empty_component() {
    // Act & Assert
    assert_eq!(Release::parse("9..4"), None);
}

#[test]
fn releases_order_by_number_not_by_text() {
    // Arrange: as text, `9.10.0` sorts before `9.4.0`.
    let older = Release::parse("9.4.0");
    let newer = Release::parse("9.10.0");

    // Act & Assert
    assert!(older < newer);
}

#[test]
fn major_is_the_first_component() {
    // Act
    let release = Release::parse("8.19.22").expect("a release");

    // Assert
    assert_eq!(release.major(), 8);
}

/// A 7.17 server installed in `/opt/es`, whose `lib/` holds `jars`.
fn installed_with(name: &str, jars: &[&str]) -> std::path::PathBuf {
    let proc = scratch_tree(name, &["812/root/opt/es/lib"]);
    write(
        &proc,
        "812/cmdline",
        "/opt/es/jdk/bin/java\0-Des.path.home=/opt/es\0-Des.path.conf=/opt/es/config\0\
         -cp\0/opt/es/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    for jar in jars {
        write(&proc, &format!("812/root/opt/es/lib/{jar}"), "");
    }
    proc
}

#[test]
fn release_skips_the_module_jars_beside_the_server_jar() {
    // Arrange: measured, `lib/` holds `elasticsearch-core-<version>.jar` and its siblings.
    let proc = installed_with(
        "elasticsearch-release-siblings",
        &[
            "elasticsearch-core-7.17.29.jar",
            "elasticsearch-7.17.29.jar",
            "elasticsearch-x-content-7.17.29.jar",
        ],
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(
        nodes[0]
            .release()
            .map(|release| release.to_string())
            .as_deref(),
        Some("7.17.29")
    );
}

#[test]
fn release_names_none_for_an_install_holding_two_server_jars() {
    // Arrange: a botched upgrade left both; which one the JVM loaded is not in the names.
    let proc = installed_with(
        "elasticsearch-release-two-jars",
        &["elasticsearch-7.17.28.jar", "elasticsearch-7.17.29.jar"],
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].release(), None);
}

#[test]
fn release_names_none_for_a_lib_holding_more_entries_than_any_install() {
    // Arrange: found by the sweep. `lib/` is inside the node's root, its owner's to fill, and
    // every entry was listed into memory as root; a real install holds a few hundred jars.
    let proc = installed_with(
        "elasticsearch-release-huge-lib",
        &["elasticsearch-7.17.29.jar"],
    );
    let lib = proc.join("812/root/opt/es/lib");
    for entry in 0..10_001 {
        std::fs::File::create(lib.join(format!("filler-{entry}"))).expect("a writable fixture");
    }

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].release(), None);
}

#[test]
fn release_names_none_for_an_install_with_no_lib() {
    // Arrange
    let proc = installed_with("elasticsearch-release-no-jar", &[]);
    std::fs::remove_dir(proc.join("812/root/opt/es/lib")).expect("an empty lib");

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].release(), None);
}

fn support_of(version: &str) -> ReleaseSupport {
    Release::parse(version).expect("a release").support()
}

#[test]
fn support_of_each_supported_release_is_supported() {
    // Act & Assert
    assert_eq!(
        support_of("7.17.29"),
        ReleaseSupport::Supported(SupportedRelease::V7_17)
    );
    assert_eq!(
        support_of("8.19.0"),
        ReleaseSupport::Supported(SupportedRelease::V8_19)
    );
    assert_eq!(
        support_of("9.4.7"),
        ReleaseSupport::Supported(SupportedRelease::V9_4)
    );
    assert_eq!(
        support_of("9.5.4"),
        ReleaseSupport::Supported(SupportedRelease::V9_5)
    );
}

#[test]
fn support_of_another_7_is_read_as_7_17() {
    // Act & Assert
    assert_eq!(
        support_of("7.10.2"),
        ReleaseSupport::ReadAs(SupportedRelease::V7_17)
    );
}

#[test]
fn support_of_another_8_is_read_as_8_19() {
    // Act & Assert
    assert_eq!(
        support_of("8.15.3"),
        ReleaseSupport::ReadAs(SupportedRelease::V8_19)
    );
}

#[test]
fn support_of_a_9_before_9_4_is_read_as_9_4() {
    // Act & Assert
    assert_eq!(
        support_of("9.2.0"),
        ReleaseSupport::ReadAs(SupportedRelease::V9_4)
    );
}

#[test]
fn support_of_a_9_after_9_5_is_read_as_9_5() {
    // Act & Assert
    assert_eq!(
        support_of("9.6.0"),
        ReleaseSupport::ReadAs(SupportedRelease::V9_5)
    );
}

#[test]
fn support_of_a_major_after_9_is_read_as_9_5() {
    // Act & Assert: the newest shape rastro knows, as the postgresql collector reads a newer major.
    assert_eq!(
        support_of("10.1.0"),
        ReleaseSupport::ReadAs(SupportedRelease::V9_5)
    );
}

#[test]
fn support_of_a_release_below_7_is_below_seven() {
    // Act & Assert
    assert_eq!(support_of("6.8.23"), ReleaseSupport::BelowSeven);
}

#[test]
fn a_supported_release_is_named_as_major_and_minor() {
    // Act & Assert
    assert_eq!(SupportedRelease::V8_19.to_string(), "8.19");
}
