//! Reading where a host fetches packages from, without needing an `/etc` to read it
//! from.
//!
//! Both apt formats are exercised, because Debian 12 ships both at once: the fixtures
//! are the shapes really found on the development box, with the URIs changed.

use std::fs;
use std::path::{Path, PathBuf};

use rastro::collectors::repositories::{
    ApkRepositories, AptSources, ArchiveType, Component, Enablement, RepositoriesCollector,
    Repository, RepositoryInventory, RepositorySet, RepositorySource, RepositorySystem, apt_deb822,
    apt_one_line,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation};

/// The deb822 shape Debian 12 ships, which expands to more than one repository.
const DEB822: &str = "\
Types: deb deb-src
URIs: mirror+file:///etc/apt/mirrors/debian.list
Suites: bookworm bookworm-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
";

/// The one-line shape a third-party repository still uses.
const ONE_LINE: &str = "\
deb [signed-by=/usr/share/keyrings/example-archive-keyring.asc] \
https://apt.example.org/pub/repos/apt bookworm-example main
";

fn tree(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("repositories-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sources.list.d")).expect("a writable scratch directory");

    root
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("a writable tree");
    fs::write(path, contents).expect("a writable file");
}

fn one_line(line: &str) -> Repository {
    apt_one_line::parse_line(line)
        .expect("this line is well formed")
        .expect("this line carries a repository")
}

fn uris_of(set: &RepositorySet) -> Vec<&str> {
    set.repositories()
        .iter()
        .map(|repository| repository.uri.as_str())
        .collect()
}

fn suites_of(set: &RepositorySet) -> Vec<&str> {
    set.repositories()
        .iter()
        .filter_map(|repository| repository.suite.as_ref())
        .map(|suite| suite.as_str())
        .collect()
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

#[test]
fn one_line_reads_every_part_of_an_entry() {
    // Act
    let repository = one_line(ONE_LINE.trim());

    // Assert
    assert_eq!(repository.archive_type, Some(ArchiveType::Binary));
    assert_eq!(
        repository.uri.as_str(),
        "https://apt.example.org/pub/repos/apt"
    );
    assert_eq!(
        repository.suite.as_ref().map(|suite| suite.as_str()),
        Some("bookworm-example")
    );
    assert_eq!(
        repository
            .components
            .iter()
            .map(Component::as_str)
            .collect::<Vec<&str>>(),
        ["main"]
    );
    assert_eq!(repository.enablement, Enablement::Enabled);
}

#[test]
fn one_line_keeps_an_option_apt_knows_as_a_setting_rather_than_a_field() {
    // Act
    let repository = one_line(ONE_LINE.trim());

    // Assert: the same repository signed by a new keyring is still that repository, so
    // the key is configuration rather than identity.
    assert_eq!(
        repository.settings.get("signed-by").map(String::as_str),
        Some("/usr/share/keyrings/example-archive-keyring.asc")
    );
}

#[test]
fn one_line_reads_an_option_list_containing_spaces() {
    // Arrange: a whitespace split would read `signed-by=...]` as the URI.
    let line = "deb [arch=amd64 signed-by=/etc/keys/x.asc] https://example.org/repo bookworm main";

    // Act
    let repository = one_line(line);

    // Assert
    assert_eq!(repository.uri.as_str(), "https://example.org/repo");
    assert_eq!(
        repository.settings.get("arch").map(String::as_str),
        Some("amd64")
    );
    assert_eq!(
        repository.settings.get("signed-by").map(String::as_str),
        Some("/etc/keys/x.asc")
    );
}

#[test]
fn one_line_accepts_brackets_spaced_away_from_their_options() {
    // Act: apt accepts this spelling too.
    let repository = one_line("deb [ arch=arm64 ] https://example.org/repo bookworm main");

    // Assert
    assert_eq!(repository.uri.as_str(), "https://example.org/repo");
    assert_eq!(
        repository.settings.get("arch").map(String::as_str),
        Some("arm64")
    );
}

#[test]
fn one_line_records_a_commented_out_entry_as_disabled() {
    // Arrange: swapping a repository is done by commenting the old line and adding a
    // new one, so dropping the old one would report a replacement as an addition.
    let line = "# deb https://old.example.org/repo bookworm main";

    // Act
    let repository = one_line(line);

    // Assert
    assert_eq!(repository.enablement, Enablement::Disabled);
    assert_eq!(repository.uri.as_str(), "https://old.example.org/repo");
}

#[test]
fn one_line_skips_a_comment_that_is_only_prose() {
    // Arrange: `sources.list` on an ordinary Debian 12 box contains nothing else.
    let line = "# See /etc/apt/sources.list.d/debian.sources";

    // Act
    let parsed = apt_one_line::parse_line(line).expect("prose is not a failure");

    // Assert: the parser decides, rather than a heuristic about what comments look
    // like.
    assert_eq!(parsed, None);
}

#[test]
fn one_line_reads_a_flat_repository_with_no_components() {
    // Act: a flat repository serves packages from one directory, with no `dists`
    // hierarchy and no components.
    let repository = one_line("deb https://example.org/repo ./");

    // Assert
    assert_eq!(
        repository.suite.as_ref().map(|suite| suite.as_str()),
        Some("./")
    );
    assert!(repository.components.is_empty());
}

#[test]
fn one_line_refuses_an_unknown_archive_type() {
    // Act: apt's parser accepts exactly `deb` and `deb-src`, so a third word means the
    // line was tokenised into the wrong slots.
    let result = apt_one_line::parse_line("rpm https://example.org/repo bookworm main");

    // Assert
    assert!(result.is_err());
}

#[test]
fn one_line_refuses_an_option_list_that_never_closes() {
    // Act & Assert
    assert!(apt_one_line::parse_line("deb [arch=amd64 https://example.org/repo bookworm").is_err());
}

#[test]
fn deb822_expands_one_paragraph_into_the_repositories_it_describes() {
    // Act: two types and two suites is four repositories, which is what apt fetches.
    let repositories = apt_deb822::parse(DEB822).expect("this paragraph is well formed");

    // Assert: recording the paragraph as written would leave the facet in the format's
    // vocabulary, so the same configuration written as one-line entries would look
    // different.
    assert_eq!(repositories.len(), 4);
    let mut described: Vec<String> = repositories
        .iter()
        .map(|repository| {
            format!(
                "{} {}",
                repository
                    .archive_type
                    .as_ref()
                    .expect("an archive type")
                    .as_str(),
                repository.suite.as_ref().expect("a suite").as_str()
            )
        })
        .collect();
    described.sort();
    assert_eq!(
        described,
        [
            "deb bookworm",
            "deb bookworm-updates",
            "deb-src bookworm",
            "deb-src bookworm-updates"
        ]
    );
}

#[test]
fn deb822_keeps_the_signing_key_as_a_setting() {
    // Act
    let repositories = apt_deb822::parse(DEB822).expect("well formed");

    // Assert
    assert_eq!(
        repositories[0]
            .settings
            .get("signed-by")
            .map(String::as_str),
        Some("/usr/share/keyrings/debian-archive-keyring.gpg")
    );
}

#[test]
fn deb822_reads_several_paragraphs() {
    // Arrange: the shipped `debian.sources` holds two, one of them for security.
    let text = format!(
        "{DEB822}\nTypes: deb\nURIs: https://security.example.org\nSuites: bookworm-security\nComponents: main\n"
    );

    // Act
    let repositories = apt_deb822::parse(&text).expect("well formed");

    // Assert
    assert_eq!(repositories.len(), 5);
}

#[test]
fn deb822_reads_a_disabled_paragraph() {
    // Arrange: this format switches a repository off with a field rather than a comment.
    let text = "Types: deb\nURIs: https://example.org/repo\nSuites: bookworm\nEnabled: no\n";

    // Act
    let repositories = apt_deb822::parse(text).expect("well formed");

    // Assert
    assert_eq!(repositories[0].enablement, Enablement::Disabled);
}

#[test]
fn deb822_folds_a_continuation_line_into_its_field() {
    // Arrange: `Signed-By` may be an entire armoured key rather than a path, and its
    // blank lines are written as a lone dot so they do not end the paragraph.
    let text = "\
Types: deb
URIs: https://example.org/repo
Suites: bookworm
Signed-By:
 -----BEGIN PGP PUBLIC KEY BLOCK-----
 .
 mDMEZ
 -----END PGP PUBLIC KEY BLOCK-----
";

    // Act
    let repositories = apt_deb822::parse(text).expect("well formed");

    // Assert
    let key = repositories[0]
        .settings
        .get("signed-by")
        .expect("the key was recorded");
    assert!(key.contains("BEGIN PGP PUBLIC KEY BLOCK"));
    assert!(
        key.contains("mDMEZ"),
        "the folded body must survive: {key:?}"
    );
    assert!(
        key.contains("\n\n"),
        "a lone dot stands for a blank line: {key:?}"
    );
}

#[test]
fn deb822_ignores_comment_lines() {
    // Arrange: in this format `#` is prose, so a comment must not start or end a paragraph.
    let text = "\
# Debian's shipped file carries comments like this
Types: deb
URIs: https://example.org/repo
# Another comment between fields
Suites: bookworm
";

    // Act
    let repositories = apt_deb822::parse(text).expect("well formed");

    // Assert
    assert_eq!(repositories.len(), 1);
    assert_eq!(
        repositories[0].suite.as_ref().map(|suite| suite.as_str()),
        Some("bookworm")
    );
}

#[test]
fn deb822_refuses_a_paragraph_with_no_types() {
    // Act: the expansion is a cross product, so an absent `Types` would quietly
    // produce no repositories and drop the paragraph from a complete facet.
    let result = apt_deb822::parse("URIs: https://example.org/repo\nSuites: bookworm\n");

    // Assert
    let failure = result.expect_err("a paragraph that describes nothing must not be dropped");
    assert!(
        failure.to_string().contains("types"),
        "the message must name the missing field, got: {failure}"
    );
}

#[test]
fn read_takes_both_formats_from_one_configuration_tree() {
    // Arrange: exactly the layout Debian 12 ships.
    let root = tree("both-formats");
    write(
        &root,
        "sources.list",
        "# See sources.list.d/debian.sources\n",
    );
    write(&root, "sources.list.d/debian.sources", DEB822);
    write(&root, "sources.list.d/example.list", ONE_LINE);

    // Act
    let set = AptSources::at(&root).read().expect("well formed");

    // Assert: four from the deb822 paragraph and one from the one-line file.
    assert_eq!(set.len(), 5);
    assert!(suites_of(&set).contains(&"bookworm-example"));
    assert!(suites_of(&set).contains(&"bookworm-updates"));
}

#[test]
fn read_ignores_a_file_apt_would_ignore() {
    // Arrange: apt reads only `.list` and `.sources`, so a `.save` left by dpkg is not
    // a repository and reporting it would put one in the facet the host does not use.
    let root = tree("ignored-extensions");
    write(&root, "sources.list.d/example.list", ONE_LINE);
    write(
        &root,
        "sources.list.d/example.list.save",
        "deb https://stale.example.org/repo bookworm main\n",
    );
    write(
        &root,
        "sources.list.d/README",
        "deb https://readme.example.org/repo bookworm main\n",
    );

    // Act
    let set = AptSources::at(&root).read().expect("well formed");

    // Assert
    assert_eq!(uris_of(&set), ["https://apt.example.org/pub/repos/apt"]);
}

#[test]
fn read_sorts_the_repositories_so_the_directory_order_never_reaches_the_facet() {
    // Arrange: two files whose alphabetical order is the reverse of their content's.
    let root = tree("sorted");
    write(
        &root,
        "sources.list.d/a-second.list",
        "deb https://zulu.example.org/repo bookworm main\n",
    );
    write(
        &root,
        "sources.list.d/b-first.list",
        "deb https://alpha.example.org/repo bookworm main\n",
    );

    // Act
    let set = AptSources::at(&root).read().expect("well formed");

    // Assert
    assert_eq!(
        uris_of(&set),
        [
            "https://alpha.example.org/repo",
            "https://zulu.example.org/repo"
        ]
    );
}

#[test]
fn read_keeps_the_same_repository_declared_twice() {
    // Arrange: apt warns about exactly this, so deduplicating would hide it.
    let root = tree("duplicated");
    write(
        &root,
        "sources.list.d/one.list",
        "deb https://example.org/repo bookworm main\n",
    );
    write(
        &root,
        "sources.list.d/two.list",
        "deb https://example.org/repo bookworm main\n",
    );

    // Act
    let set = AptSources::at(&root).read().expect("well formed");

    // Assert
    assert_eq!(set.len(), 2);
}

#[test]
fn read_names_the_file_a_failure_came_from() {
    // Arrange: one bad line among many files, and an operator needs to know which.
    let root = tree("named-failure");
    write(
        &root,
        "sources.list.d/broken.list",
        "rpm https://x/y z main\n",
    );

    // Act
    let result = AptSources::at(&root).read();

    // Assert
    let failure = result.expect_err("an unknown archive type must fail");
    assert!(
        failure.to_string().contains("broken.list"),
        "the message must name the file, got: {failure}"
    );
}

#[test]
fn apk_reads_a_bare_uri_as_a_repository() {
    // Act: apk divides a repository neither by release nor by component.
    let set = ApkRepositories::parse("https://dl-cdn.example.org/alpine/v3.19/main\n")
        .expect("well formed");

    // Assert
    let repository = &set.repositories()[0];
    assert_eq!(
        repository.uri.as_str(),
        "https://dl-cdn.example.org/alpine/v3.19/main"
    );
    assert_eq!(repository.archive_type, None);
    assert_eq!(repository.suite, None);
    assert!(repository.components.is_empty());
}

#[test]
fn apk_reads_a_tagged_repository() {
    // Act: a tag is how an Alpine box pulls one package from a newer branch without
    // moving the whole system onto it.
    let set = ApkRepositories::parse("@edge https://dl-cdn.example.org/alpine/edge/main\n")
        .expect("well formed");

    // Assert
    let repository = &set.repositories()[0];
    assert_eq!(
        repository.tag.as_ref().map(|tag| tag.as_str()),
        Some("edge")
    );
    assert_eq!(
        repository.uri.as_str(),
        "https://dl-cdn.example.org/alpine/edge/main"
    );
}

#[test]
fn apk_records_a_commented_repository_as_disabled() {
    // Act
    let set = ApkRepositories::parse("#https://dl-cdn.example.org/alpine/v3.19/community\n")
        .expect("well formed");

    // Assert
    assert_eq!(set.repositories()[0].enablement, Enablement::Disabled);
}

#[test]
fn apk_refuses_a_tag_with_no_uri_after_it() {
    // Act & Assert
    assert!(ApkRepositories::parse("@edge\n").is_err());
}

#[test]
fn the_inventory_names_every_system_rastro_can_read() {
    // Arrange: only apt was found.
    let root = tree("inventory");
    write(&root, "sources.list.d/example.list", ONE_LINE);
    let found = vec![(
        RepositorySystem::Apt,
        AptSources::at(&root).read().expect("well formed"),
    )];

    // Act
    let observation = Observation::from(&RepositoryInventory::new(found).expect("one system"));

    // Assert: a document saying nothing about apk cannot be told apart from one written
    // before rastro could read apk, so the key is there with null under it.
    let keys: Vec<String> = object_of(&observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, ["apk", "apt"]);
    assert_eq!(
        field(&observation, "apk").content(),
        &Content::Scalar(rastro_fingerprint::Scalar::Null)
    );
}

#[test]
fn the_inventory_refuses_a_system_reported_twice() {
    // Arrange
    let found = vec![
        (RepositorySystem::Apt, RepositorySet::default()),
        (RepositorySystem::Apt, RepositorySet::default()),
    ];

    // Act & Assert
    assert!(RepositoryInventory::new(found).is_err());
}

#[test]
fn detect_finds_nothing_when_neither_system_is_configured() {
    // Arrange: a root with no apt tree and no apk file.
    let root = tree("undetected");

    // Act
    let sources = AptSources::at(root.join("absent"));

    // Assert
    assert!(!sources.root().is_dir());
}

#[test]
fn presence_is_always_present_because_the_subject_is_what_rastro_can_read() {
    // Act & Assert: `absent` would claim the host fetches packages from nowhere, which
    // two negative probes cannot establish.
    assert_eq!(
        RepositoriesCollector::reading(Vec::new()).presence(),
        Presence::Present
    );
}

#[test]
fn collect_reports_a_key_per_system_even_with_nothing_found() {
    // Act
    let collected = RepositoriesCollector::reading(Vec::new())
        .collect()
        .expect("reporting nothing found is not a failure");

    // Assert
    let keys: Vec<String> = object_of(&collected)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, ["apk", "apt"]);
}

#[test]
fn collect_reads_the_system_it_was_given() {
    // Arrange
    let root = tree("collect");
    write(&root, "sources.list.d/example.list", ONE_LINE);

    // Act
    let collected =
        RepositoriesCollector::reading(vec![RepositorySource::Apt(AptSources::at(&root))])
            .collect()
            .expect("well formed");

    // Assert
    let apt = field(&collected, "apt");
    match apt.content() {
        Content::List(items) => assert_eq!(items.len(), 1),
        other => panic!("expected a list of repositories, got {other:?}"),
    }
}
