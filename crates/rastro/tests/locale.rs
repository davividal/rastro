//! Reading the host's localisation, without needing an `/etc` to read it from.

use std::fs;
use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::locale::{LocaleCollector, LocaleFiles, SettingValue};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar};
use support::fs_tree::scratch_tree;
use support::observation::{field, object_of};

fn tree(name: &str) -> PathBuf {
    scratch_tree(&format!("locale-{name}"), &[])
}

/// The source over one scratch file, standing in for `/etc/default/locale`.
fn files_in(root: &Path, names: &[&str]) -> LocaleFiles {
    LocaleFiles::at(names.iter().map(|name| root.join(name)))
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn read_reads_a_settings_file() {
    // Arrange: `/etc/default/locale` on the development box holds exactly this.
    let root = tree("one-file");
    fs::write(root.join("locale"), "LANG=C.UTF-8\n").expect("a writable file");

    // Act
    let observation = Observation::from(
        &files_in(&root, &["locale"])
            .read()
            .expect("this file is well formed"),
    );

    // Assert
    let file = field(&observation, root.join("locale").to_str().expect("utf-8"));
    assert_eq!(text(&field(&file, "LANG")), "C.UTF-8");
}

#[test]
fn read_strips_the_quotes_a_shell_fragment_may_use() {
    // Arrange: these files are sourced by shell scripts, so both spellings set the same
    // locale and keeping the quotes would make one box differ from another over punctuation.
    let root = tree("quoted");
    fs::write(
        root.join("locale"),
        "LANG=\"en_GB.UTF-8\"\nLC_TIME='en_IE.UTF-8'\n",
    )
    .expect("a writable file");

    // Act
    let observation = Observation::from(&files_in(&root, &["locale"]).read().expect("well formed"));
    let file = field(&observation, root.join("locale").to_str().expect("utf-8"));

    // Assert
    assert_eq!(text(&field(&file, "LANG")), "en_GB.UTF-8");
    assert_eq!(text(&field(&file, "LC_TIME")), "en_IE.UTF-8");
}

#[test]
fn a_value_with_a_quote_on_one_side_only_is_left_alone() {
    // Act: that is a broken file rather than a quoted value, and half-repairing it would
    // hide the breakage.
    let value = SettingValue::new("\"en_GB.UTF-8");

    // Assert
    assert_eq!(value.as_str(), "\"en_GB.UTF-8");
}

#[test]
fn read_keeps_an_explicitly_empty_setting() {
    // Arrange: `LANG=` is legal and means the variable is explicitly unset, which is not the
    // same as the variable being absent.
    let root = tree("empty-value");
    fs::write(root.join("locale"), "LANG=\n").expect("a writable file");

    // Act
    let observation = Observation::from(&files_in(&root, &["locale"]).read().expect("well formed"));
    let file = field(&observation, root.join("locale").to_str().expect("utf-8"));

    // Assert
    assert_eq!(
        field(&file, "LANG").content(),
        &Content::Scalar(Scalar::Text(String::new()))
    );
}

#[test]
fn read_skips_comments_and_blank_lines() {
    // Arrange: a `#` line is a comment to the shell that sources these files too.
    let root = tree("comments");
    fs::write(
        root.join("locale"),
        "# set by locales postinst\n\nLANG=C.UTF-8\n",
    )
    .expect("a writable file");

    // Act
    let observation = Observation::from(&files_in(&root, &["locale"]).read().expect("well formed"));
    let file = field(&observation, root.join("locale").to_str().expect("utf-8"));

    // Assert
    assert_eq!(object_of(&file).len(), 1);
}

#[test]
fn read_skips_a_line_that_sets_nothing() {
    // Arrange: these files are sourced by shell scripts, so a bare `export` is legal and
    // refusing it would lose the whole facet over a line that sets nothing.
    let root = tree("no-assignment");
    fs::write(root.join("locale"), "export\nLANG=C.UTF-8\n").expect("a writable file");

    // Act
    let observation = Observation::from(&files_in(&root, &["locale"]).read().expect("well formed"));
    let file = field(&observation, root.join("locale").to_str().expect("utf-8"));

    // Assert
    assert_eq!(object_of(&file).len(), 1);
}

#[test]
fn read_refuses_a_setting_written_twice() {
    // Arrange: the shell would take the last, so the file is ambiguous about what the box is
    // set to, and resolving it quietly would hide a misconfiguration.
    let root = tree("duplicated");
    fs::write(root.join("locale"), "LANG=C.UTF-8\nLANG=en_GB.UTF-8\n").expect("a writable file");

    // Act
    let result = files_in(&root, &["locale"]).read();

    // Assert
    let failure = result.expect_err("an ambiguous file must not be resolved silently");
    assert!(
        failure.to_string().contains("LANG"),
        "the message must name the setting, got: {failure}"
    );
}

#[test]
fn read_reports_a_file_that_is_not_there_as_absent_rather_than_omitting_it() {
    // Arrange: `/etc/locale.conf` is systemd's and a Debian box usually has only Debian's.
    let root = tree("absent-file");
    fs::write(root.join("locale"), "LANG=C.UTF-8\n").expect("a writable file");

    // Act
    let observation = Observation::from(
        &files_in(&root, &["locale.conf", "locale"])
            .read()
            .expect("an absent file is not a failure"),
    );

    // Assert: a document silent about the file cannot be told from one written before rastro
    // read it, while one saying it is not there states a fact about the host.
    assert_eq!(
        field(
            &observation,
            root.join("locale.conf").to_str().expect("utf-8")
        )
        .content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn read_reads_every_file_it_was_given() {
    // Arrange: the keymap lives in its own file, and which file a setting is in changes what
    // it does.
    let root = tree("several-files");
    fs::write(root.join("locale"), "LANG=C.UTF-8\n").expect("a writable file");
    fs::write(root.join("vconsole.conf"), "KEYMAP=uk\nFONT=Lat15\n").expect("a writable file");

    // Act
    let observation = Observation::from(
        &files_in(&root, &["locale", "vconsole.conf"])
            .read()
            .expect("well formed"),
    );

    // Assert
    assert_eq!(object_of(&observation).len(), 2);
    let vconsole = field(
        &observation,
        root.join("vconsole.conf").to_str().expect("utf-8"),
    );
    assert_eq!(text(&field(&vconsole, "KEYMAP")), "uk");
}

#[test]
fn read_keeps_the_case_a_setting_was_written_in() {
    // Arrange: a variable the shell would not export because somebody wrote it in lower case
    // is a real misconfiguration, and folding the case would hide it.
    let root = tree("case");
    fs::write(root.join("locale"), "lang=C.UTF-8\n").expect("a writable file");

    // Act
    let observation = Observation::from(&files_in(&root, &["locale"]).read().expect("well formed"));
    let file = field(&observation, root.join("locale").to_str().expect("utf-8"));

    // Assert
    assert_eq!(
        object_of(&file)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<String>>(),
        ["lang"]
    );
}

#[test]
fn the_default_source_names_the_three_files_that_matter() {
    // Act: systemd's, Debian's, and the virtual console's.
    let files = LocaleFiles::new();

    // Assert
    let paths: Vec<String> = files
        .paths()
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "/etc/locale.conf",
            "/etc/default/locale",
            "/etc/vconsole.conf"
        ]
    );
}

#[test]
fn presence_is_always_present_because_a_box_with_no_files_falls_back_to_the_c_locale() {
    // Act & Assert: that is not an absence of localisation, and the data says every file is
    // absent, which is the honest answer.
    let root = tree("presence");
    assert_eq!(
        LocaleCollector::reading(files_in(&root, &["locale"])).presence(),
        Presence::Present
    );
}

#[test]
fn collect_reports_every_file_even_when_none_of_them_exists() {
    // Arrange
    let root = tree("none-exist");

    // Act
    let collected = LocaleCollector::reading(files_in(&root, &["locale", "vconsole.conf"]))
        .collect()
        .expect("absent files are not a failure");

    // Assert
    assert_eq!(object_of(&collected).len(), 2);
}
