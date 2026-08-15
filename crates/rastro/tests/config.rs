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
