//! Assembling a configuration the way nginx assembles it, without asking nginx.
//!
//! The include rules asserted here were measured against nginx 1.30: a relative include
//! resolves against the prefix, a glob is read in sorted order, a glob that matches nothing
//! is not an error, and a literal include of a file that is not there is.

use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::nginx::model::{Configuration, ConfigurationFile};
use rastro::collectors::nginx::source::ConfigurationFiles;
use rastro::collectors::nginx::value_objects::FileReading;
use support::fs_tree::{scratch_tree, write};

/// The shape Debian ships: a top-level file that includes a directory of vhosts.
const ROOT: &str = "\
user nginx;
http {
    include conf.d/*.conf;
}
";

fn tree(name: &str) -> PathBuf {
    scratch_tree(&format!("nginx-{name}"), &["conf.d"])
}

fn read(prefix: &Path) -> Configuration {
    ConfigurationFiles::at(prefix.join("nginx.conf"), prefix)
        .expect("the scratch tree is absolute")
        .read()
}

fn paths_of(configuration: &Configuration) -> Vec<String> {
    configuration
        .files
        .iter()
        .map(|file| {
            Path::new(file.path.as_str())
                .file_name()
                .expect("a fixture file has a name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn digest_of(file: &ConfigurationFile) -> String {
    match &file.reading {
        FileReading::Parsed { digest } => digest.as_str(),
        FileReading::Refused { reason } => panic!("expected a parsed file, got {reason:?}"),
    }
}

fn refusal_of(file: &ConfigurationFile) -> String {
    match &file.reading {
        FileReading::Parsed { digest } => panic!("expected a refusal, got {digest:?}"),
        FileReading::Refused { reason } => reason.as_str().to_owned(),
    }
}

/// The names of the directives one level under `http`, which is where an include lands them.
fn inside_http(configuration: &Configuration) -> Vec<&str> {
    configuration
        .directives
        .iter()
        .find(|directive| directive.name.as_str() == "http")
        .and_then(|http| http.block.as_ref())
        .expect("the fixture has an http block")
        .iter()
        .map(|directive| directive.name.as_str())
        .collect()
}

#[test]
fn an_include_brings_in_the_directives_of_the_file_it_names() {
    // Arrange
    let prefix = tree("include");
    write(&prefix, "nginx.conf", ROOT);
    write(&prefix, "conf.d/site.conf", "server { listen 80; }\n");

    // Act
    let configuration = read(&prefix);

    // Assert: the server block sits where the include did, not beside it.
    assert_eq!(inside_http(&configuration), ["server"]);
}

#[test]
fn a_glob_include_reads_every_match_in_sorted_order() {
    // Arrange: written in an order that is not the sorted one.
    let prefix = tree("glob-order");
    write(&prefix, "nginx.conf", ROOT);
    for name in ["c_third", "a_first", "b_second"] {
        write(
            &prefix,
            &format!("conf.d/{name}.conf"),
            "server { listen 80; }\n",
        );
    }

    // Act
    let configuration = read(&prefix);

    // Assert
    assert_eq!(
        paths_of(&configuration),
        [
            "nginx.conf",
            "a_first.conf",
            "b_second.conf",
            "c_third.conf"
        ]
    );
}

#[test]
fn a_glob_that_matches_nothing_is_not_a_failure() {
    // Arrange
    let prefix = tree("glob-empty");
    write(&prefix, "nginx.conf", ROOT);

    // Act
    let configuration = read(&prefix);

    // Assert
    assert_eq!(paths_of(&configuration), ["nginx.conf"]);
    assert_eq!(inside_http(&configuration), Vec::<&str>::new());
}

#[test]
fn a_literal_include_of_a_file_that_is_not_there_is_recorded() {
    // Arrange: nginx refuses to start on this, so it is state worth reporting rather than
    // something to pass over.
    let prefix = tree("include-missing");
    write(&prefix, "nginx.conf", "include conf.d/absent.conf;\n");

    // Act
    let configuration = read(&prefix);

    // Assert
    assert_eq!(paths_of(&configuration), ["nginx.conf", "absent.conf"]);
    assert!(
        refusal_of(&configuration.files[1]).contains("No such file"),
        "{}",
        refusal_of(&configuration.files[1])
    );
}

#[test]
fn a_file_the_grammar_refuses_costs_only_that_file() {
    // Arrange
    let prefix = tree("bad-syntax");
    write(&prefix, "nginx.conf", ROOT);
    write(&prefix, "conf.d/broken.conf", "server { listen 80;\n");
    write(&prefix, "conf.d/sound.conf", "server { listen 81; }\n");

    // Act
    let configuration = read(&prefix);

    // Assert
    assert!(refusal_of(&configuration.files[1]).contains("never closed"));
    assert_eq!(inside_http(&configuration), ["server"]);
}

#[test]
fn a_digest_is_taken_of_what_the_file_says_rather_than_how_it_is_written() {
    // Arrange: the same configuration, commented and spaced differently.
    let terse = tree("digest-terse");
    write(
        &terse,
        "nginx.conf",
        "user nginx;\nhttp { server { listen 80; } }\n",
    );
    let verbose = tree("digest-verbose");
    write(
        &verbose,
        "nginx.conf",
        "# the account the workers drop to\nuser   nginx;\n\nhttp {\n    server {\n        listen  80;\n    }\n}\n",
    );

    // Act
    let terse = read(&terse);
    let verbose = read(&verbose);

    // Assert
    assert_eq!(digest_of(&terse.files[0]), digest_of(&verbose.files[0]));
}

#[test]
fn a_digest_changes_when_a_directive_does() {
    // Arrange
    let before = tree("digest-before");
    write(&before, "nginx.conf", "server { listen 80; }\n");
    let after = tree("digest-after");
    write(&after, "nginx.conf", "server { listen 8080; }\n");

    // Act & Assert
    assert_ne!(
        digest_of(&read(&before).files[0]),
        digest_of(&read(&after).files[0])
    );
}

#[test]
fn an_include_cycle_is_refused_rather_than_followed() {
    // Arrange
    let prefix = tree("cycle");
    write(&prefix, "nginx.conf", "include conf.d/loop.conf;\n");
    write(&prefix, "conf.d/loop.conf", "include ../nginx.conf;\n");

    // Act
    let configuration = read(&prefix);

    // Assert
    assert!(
        refusal_of(&configuration.files[2]).contains("cycle"),
        "{}",
        refusal_of(&configuration.files[2])
    );
}

#[test]
fn an_absolute_include_is_taken_as_it_stands() {
    // Arrange
    let prefix = tree("absolute");
    let included = prefix.join("conf.d/site.conf");
    write(
        &prefix,
        "nginx.conf",
        &format!("http {{ include {}; }}\n", included.display()),
    );
    write(&prefix, "conf.d/site.conf", "server { listen 80; }\n");

    // Act
    let configuration = read(&prefix);

    // Assert
    assert_eq!(inside_http(&configuration), ["server"]);
}
