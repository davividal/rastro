//! Which release a server jar's name says, and when it says none.

use rastro::collectors::elasticsearch::{Release, ResidentNode};

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
fn release_names_none_for_an_install_with_no_lib() {
    // Arrange
    let proc = installed_with("elasticsearch-release-no-jar", &[]);
    std::fs::remove_dir(proc.join("812/root/opt/es/lib")).expect("an empty lib");

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].release(), None);
}
