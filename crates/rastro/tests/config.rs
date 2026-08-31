//! Reading a config, without needing a file to read it from.

use rastro::config::Config;

#[test]
fn a_config_with_no_exclusions_excludes_nothing() {
    // Act
    let config = Config::parse("").expect("an empty config is valid");

    // Assert
    assert!(config.excluded().is_empty());
}

#[test]
fn parse_reads_the_exclusion_list() {
    // Act
    let config = Config::parse("[collectors]\nexclude = [\"mounts\", \"nginx\"]\n")
        .expect("this config is well formed");

    // Assert
    assert_eq!(config.excluded(), ["mounts", "nginx"]);
}

#[test]
fn parse_accepts_a_collectors_table_with_no_exclusions() {
    // Act
    let config = Config::parse("[collectors]\n").expect("an empty table is valid");

    // Assert
    assert!(config.excluded().is_empty());
}

#[test]
fn parse_refuses_a_misspelled_key() {
    // Act: `excludes` is not `exclude`, and silently doing nothing would leave
    // the operator believing a collector was switched off.
    let result = Config::parse("[collectors]\nexcludes = [\"mounts\"]\n");

    // Assert
    let failure = result.expect_err("an unknown key must not be ignored");
    assert!(
        failure.to_string().contains("excludes"),
        "the message must name the key, got: {failure}"
    );
}

#[test]
fn parse_refuses_a_misspelled_table() {
    // Act
    let result = Config::parse("[collector]\nexclude = [\"mounts\"]\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_refuses_malformed_toml() {
    // Act
    let result = Config::parse("[collectors\nexclude =");

    // Assert
    assert!(result.is_err());
}

#[test]
fn the_default_config_excludes_nothing() {
    // Act & Assert: running with no `--config` is the same as running with an
    // empty one, which is what makes the tool work on a box you know nothing
    // about.
    assert!(Config::default().excluded().is_empty());
}

#[test]
fn parse_deduplicates_and_orders_the_exclusion_list() {
    // Act
    let config = Config::parse("[collectors]\nexclude = [\"nginx\", \"mounts\", \"nginx\"]\n")
        .expect("this config is well formed");

    // Assert: two configs meaning the same thing must produce the same envelope,
    // in the field whose whole purpose is making runs comparable.
    assert_eq!(config.excluded(), ["mounts", "nginx"]);
}

#[test]
fn two_configs_meaning_the_same_thing_are_equal() {
    // Act
    let one = Config::parse("[collectors]\nexclude = [\"a\", \"b\"]\n").expect("well formed");
    let other =
        Config::parse("[collectors]\nexclude = [\"b\", \"a\", \"b\"]\n").expect("well formed");

    // Assert
    assert_eq!(one.excluded(), other.excluded());
}

#[test]
fn a_malformed_config_names_the_file_it_could_not_parse() {
    // Arrange
    let path = std::env::temp_dir().join("rastro-malformed.toml");
    std::fs::write(&path, "[collectors\nexclude =").expect("the temp dir should be writable");

    // Act
    let failure = Config::load(&path).expect_err("this config is not valid TOML");

    // Assert: an operator with a broken file needs to know which file.
    assert!(
        failure.to_string().contains("rastro-malformed.toml"),
        "the message must name the file, got: {failure}"
    );
}

#[test]
fn load_records_the_path_it_read() {
    // Arrange
    let path = std::env::temp_dir().join("rastro-source.toml");
    std::fs::write(&path, "[collectors]\n").expect("the temp dir should be writable");

    // Act
    let config = Config::load(&path).expect("this config is well formed");

    // Assert: provenance. Which file configured a run is a real difference
    // between two runs, and it is not a secret.
    assert_eq!(
        config.source(),
        Some(path.to_str().expect("a UTF-8 temp path"))
    );
}

#[test]
fn the_default_config_has_no_source() {
    // Act & Assert
    assert_eq!(Config::default().source(), None);
}

#[cfg(unix)]
#[test]
fn load_refuses_a_path_it_could_not_record_faithfully() {
    // Arrange: a legal path that is not valid UTF-8.
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let path = std::path::PathBuf::from(OsStr::from_bytes(b"/tmp/rastro-\xff.toml"));

    // Act
    let result = Config::load(&path);

    // Assert: recording it lossily would put a path into the document that was
    // never on the box, in a document whose point is exactness.
    let failure = result.expect_err("an unrecordable path must not be guessed at");
    assert!(
        failure.to_string().contains("UTF-8"),
        "the message must say why, got: {failure}"
    );
}

#[test]
fn a_config_can_narrow_the_walk_over_trees_it_names() {
    // Arrange: the gap this closes. Until now the only lever over which trees the walk reads
    // was a collector's claim, resolved from the host — so a runaway walk was unfixable
    // without rebuilding the binary, and CI could not tell rastro that its own build
    // directory is noise.
    let config = Config::parse(
        r#"
[filesystem]
metadata_only = ["/srv/media"]
churns = ["/home/runner/actions-runner"]
sealed = ["/home/runner/work/rastro/target"]
"#,
    )
    .expect("three narrowings are a legal config");

    // Act & Assert
    assert_eq!(config.walk_metadata_only(), ["/srv/media"]);
    assert_eq!(config.walk_churns(), ["/home/runner/actions-runner"]);
    assert_eq!(config.walk_sealed(), ["/home/runner/work/rastro/target"]);
}

#[test]
fn a_config_cannot_ask_for_content_to_be_hashed() {
    // Act
    let refused = Config::parse(
        r#"
[filesystem]
hashed = ["/etc"]
"#,
    );

    // Assert: the three keys are all narrowings, and there is deliberately no fourth. A
    // config that could widen the walk would be an inclusion list in a tool whose premise is
    // that an operator cannot enumerate what nobody documented — and hashing is what turned
    // a fingerprint into a 51-minute disk read.
    assert!(refused.is_err());
}

#[test]
fn the_walk_narrowings_are_sorted_and_deduplicated() {
    // Arrange
    let config = Config::parse(
        r#"
[filesystem]
churns = ["/var/spool", "/opt/cache", "/var/spool"]
"#,
    )
    .expect("a legal config");

    // Act & Assert: two configs meaning the same thing must produce the same document, for
    // the same reason the collector exclusions are sorted.
    assert_eq!(config.walk_churns(), ["/opt/cache", "/var/spool"]);
}

#[test]
fn a_config_with_no_filesystem_section_narrows_nothing() {
    // Act
    let config = Config::parse("[collectors]\nexclude = [\"mounts\"]\n").expect("a legal config");

    // Assert
    assert!(config.walk_metadata_only().is_empty());
    assert!(config.walk_churns().is_empty());
    assert!(config.walk_sealed().is_empty());
}
