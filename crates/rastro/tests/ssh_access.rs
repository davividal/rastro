//! Reading who can log in over ssh, without needing an `sshd` to run.
//!
//! Every account and every key below is invented. The box this was developed against carries
//! fifty-odd real people's authorized keys, and a fixture is a file in a public repository.

use std::fs;
use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::ssh_access::{
    SshAccessCollector, SshFiles, SshServer, Sshd, authorized_keys, resolve,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar};
use support::fs_tree::scratch_tree;
use support::observation::{field, items_of, object_of};

/// The lines of `sshd -T` this collector reads, as the development box reports them.
const SSHD_DUMP: &str = "\
permitrootlogin without-password
pubkeyauthentication yes
passwordauthentication no
authorizedkeyscommand none
authorizedkeyscommanduser none
authorizedkeysfile .ssh/authorized_keys .ssh/authorized_keys2
";

/// A key body that is obviously not a real one, kept short so the fixtures stay readable.
const ED25519: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIExampleKeyBodyNotReal0000000000000";

fn server() -> SshServer {
    Sshd::parse(SSHD_DUMP).expect("this dump is well formed")
}

fn tree(name: &str) -> PathBuf {
    scratch_tree(&format!("ssh-access-{name}"), &[])
}

fn fake_sshd(name: &str) -> Sshd {
    let root = tree(&format!("fake-sshd-{name}"));
    let script = root.join("sshd");
    fs::write(&script, format!("#!/bin/sh\nprintf '%s' '{SSHD_DUMP}'\n"))
        .expect("a writable script");
    let mut permissions = fs::metadata(&script).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o700);
    }
    fs::set_permissions(&script, permissions).expect("an executable script");
    Sshd::using(
        rastro::collectors::canonical_tool::CanonicalTool::located_in(
            "sshd",
            &[root.to_str().expect("utf-8 scratch path")],
        )
        .expect("the fake tool should be locatable"),
    )
}

fn files(sshd_name: &str, passwd: &Path) -> SshFiles {
    SshFiles::using(fake_sshd(sshd_name), passwd)
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn sshd_reads_the_settings_that_decide_whether_a_key_matters() {
    // Act
    let server = server();

    // Assert
    assert_eq!(server.permit_root_login.as_str(), "without-password");
    assert_eq!(server.password_authentication.as_str(), "no");
    assert_eq!(server.public_key_authentication.as_str(), "yes");
    assert_eq!(server.authorized_keys_command.as_str(), "none");
}

#[test]
fn sshd_keeps_permit_root_login_as_a_word_rather_than_a_boolean() {
    // Arrange: its four legal values are `yes`, `no`, `prohibit-password` and
    // `forced-commands-only`, and only the first two are booleans.
    let prohibited = SSHD_DUMP.replace("without-password", "prohibit-password");

    // Act
    let server = Sshd::parse(&prohibited).expect("well formed");

    // Assert: flattening this would erase the difference between root logging in with a key
    // and root not logging in at all.
    assert_eq!(server.permit_root_login.as_str(), "prohibit-password");
}

#[test]
fn sshd_reads_every_key_file_pattern_in_the_order_it_searches_them() {
    // Act
    let server = server();

    // Assert
    assert_eq!(
        server.authorized_keys_files,
        [".ssh/authorized_keys", ".ssh/authorized_keys2"]
    );
}

#[test]
fn sshd_refuses_a_dump_missing_a_setting() {
    // Act: `sshd -T` prints all of them because it prints its own defaults, so a missing one
    // means the output is not what rastro believes.
    let result = Sshd::parse("permitrootlogin no\n");

    // Assert
    let failure = result.expect_err("a missing setting must not be defaulted");
    assert!(
        failure.to_string().contains("passwordauthentication"),
        "the message must name the missing setting, got: {failure}"
    );
}

#[test]
fn keys_parses_a_line_with_no_options() {
    // Act
    let keys = authorized_keys::parse(&format!("ssh-ed25519 {ED25519} operator@laptop\n"))
        .expect("this file is well formed");

    // Assert
    assert_eq!(keys[0].key_type.as_str(), "ssh-ed25519");
    assert_eq!(keys[0].key.as_str(), ED25519);
    assert_eq!(keys[0].comment.as_str(), "operator@laptop");
    assert!(keys[0].options.is_empty());
}

#[test]
fn keys_parses_a_line_with_no_comment() {
    // Act: this has the same field count as an options-bearing key with one comment, which is
    // why the dialect is decided by looking at the first field rather than by counting.
    let keys = authorized_keys::parse(&format!("ssh-ed25519 {ED25519}\n")).expect("well formed");

    // Assert
    assert_eq!(keys[0].comment.as_str(), "");
    assert!(keys[0].options.is_empty());
}

#[test]
fn keys_parses_the_options_at_the_front_of_a_line() {
    // Act: an option disappearing is a privilege escalation that leaves the key untouched.
    let keys = authorized_keys::parse(&format!(
        "no-port-forwarding,no-agent-forwarding ssh-ed25519 {ED25519} operator@laptop\n"
    ))
    .expect("well formed");

    // Assert: sorted, so reordering a line does not read as a change.
    assert_eq!(
        keys[0]
            .options
            .iter()
            .map(|option| option.as_str())
            .collect::<Vec<&str>>(),
        ["no-agent-forwarding", "no-port-forwarding"]
    );
}

#[test]
fn keys_does_not_split_a_quoted_option_on_its_commas() {
    // Act: the same hazard the mount collector's option splitter has. A naive split invents
    // two options nobody authorised.
    let keys = authorized_keys::parse(&format!(
        "command=\"/usr/bin/thing --list a,b\",from=\"10.0.0.0/8\" ssh-ed25519 {ED25519} op\n"
    ))
    .expect("well formed");

    // Assert
    assert_eq!(
        keys[0]
            .options
            .iter()
            .map(|option| option.as_str())
            .collect::<Vec<&str>>(),
        [
            "command=\"/usr/bin/thing --list a,b\"",
            "from=\"10.0.0.0/8\""
        ]
    );
}

#[test]
fn keys_keeps_a_comment_with_spaces_in_it_whole() {
    // Act: OpenSSH does not tokenise the comment, so this is one comment and not three fields.
    let keys = authorized_keys::parse(&format!("ssh-ed25519 {ED25519} operator's laptop, 2026\n"))
        .expect("well formed");

    // Assert
    assert_eq!(keys[0].comment.as_str(), "operator's laptop, 2026");
}

#[test]
fn keys_skips_comments_and_blank_lines() {
    // Act
    let keys = authorized_keys::parse(&format!(
        "# a retired key\n\nssh-ed25519 {ED25519} operator@laptop\n"
    ))
    .expect("well formed");

    // Assert
    assert_eq!(keys.len(), 1);
}

#[test]
fn keys_refuses_a_line_with_no_key_after_its_type() {
    // Act
    let result = authorized_keys::parse("ssh-ed25519\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn a_failure_does_not_quote_a_whole_key_body() {
    // Act: a public key is not a secret, but an eighty-character blob on stderr buries the
    // reason the line failed.
    let long = "x".repeat(200);
    let failure = authorized_keys::parse(&format!("weird-option-list {long}"))
        .expect_err("a line with no key must fail")
        .to_string();

    // Assert
    assert!(
        failure.len() < 150,
        "the message must stay short: {failure}"
    );
    assert!(
        failure.contains("..."),
        "and say it was shortened: {failure}"
    );
}

#[test]
fn resolve_puts_a_relative_pattern_under_the_home_directory() {
    // Act: this is what makes the default `.ssh/authorized_keys` mean what everybody assumes.
    let path = resolve(
        ".ssh/authorized_keys",
        "operator",
        Path::new("/home/operator"),
    );

    // Assert
    assert_eq!(path, PathBuf::from("/home/operator/.ssh/authorized_keys"));
}

#[test]
fn resolve_uses_an_absolute_pattern_as_it_stands() {
    // Act: how a box that centralises keys works.
    let path = resolve(
        "/etc/ssh/authorized_keys/%u",
        "operator",
        Path::new("/home/operator"),
    );

    // Assert
    assert_eq!(path, PathBuf::from("/etc/ssh/authorized_keys/operator"));
}

#[test]
fn resolve_expands_the_home_token() {
    // Act
    let path = resolve("%h/.ssh/keys", "operator", Path::new("/home/operator"));

    // Assert
    assert_eq!(path, PathBuf::from("/home/operator/.ssh/keys"));
}

#[test]
fn read_accounts_finds_keys_across_every_pattern() {
    // Arrange: sshd searches both patterns, so a key in either counts.
    let root = tree("both-patterns");
    let home = root.join("home/operator");
    fs::create_dir_all(home.join(".ssh")).expect("a writable home");
    fs::write(
        home.join(".ssh/authorized_keys"),
        format!("ssh-ed25519 {ED25519} first\n"),
    )
    .expect("a writable file");
    fs::write(
        home.join(".ssh/authorized_keys2"),
        format!("ssh-rsa {ED25519} second\n"),
    )
    .expect("a writable file");
    let passwd = root.join("passwd");
    fs::write(
        &passwd,
        format!("operator:x:1000:1000::{}:/bin/sh\n", home.display()),
    )
    .expect("a writable passwd");

    // Act
    let files = rastro::collectors::ssh_access::SshFiles::using(
        Sshd::using(
            rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
                .expect("every unix has /bin/sh"),
        ),
        &passwd,
    );
    let accounts = files
        .read_accounts(&server())
        .expect("this tree is well formed");

    // Assert
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].0, "operator");
    assert_eq!(accounts[0].1.len(), 2);
}

#[test]
fn read_accounts_reports_only_accounts_that_have_a_key_file() {
    // Arrange: every system account would otherwise appear with an empty list, drowning the
    // handful that matter.
    let root = tree("only-with-keys");
    let home = root.join("home/operator");
    fs::create_dir_all(home.join(".ssh")).expect("a writable home");
    fs::write(
        home.join(".ssh/authorized_keys"),
        format!("ssh-ed25519 {ED25519} operator\n"),
    )
    .expect("a writable file");
    let passwd = root.join("passwd");
    fs::write(
        &passwd,
        format!(
            "operator:x:1000:1000::{}:/bin/sh\ndaemon:x:1:1::/nonexistent:/usr/sbin/nologin\n",
            home.display()
        ),
    )
    .expect("a writable passwd");

    // Act
    let files = rastro::collectors::ssh_access::SshFiles::using(
        Sshd::using(
            rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
                .expect("every unix has /bin/sh"),
        ),
        &passwd,
    );
    let accounts = files.read_accounts(&server()).expect("well formed");

    // Assert
    assert_eq!(
        accounts
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<&str>>(),
        ["operator"]
    );
}

#[test]
fn read_translates_the_server_and_account_files_together() {
    let root = tree("read");
    let home = root.join("home/operator");
    fs::create_dir_all(home.join(".ssh")).expect("a writable home");
    fs::write(
        home.join(".ssh/authorized_keys"),
        format!("ssh-ed25519 {ED25519} operator\n"),
    )
    .expect("a writable file");
    let passwd = root.join("passwd");
    fs::write(
        &passwd,
        format!("operator:x:1000:1000::{}:/bin/sh\n", home.display()),
    )
    .expect("a writable passwd");

    let access = files("read", &passwd)
        .read()
        .expect("this tree is well formed");
    let observation = Observation::from(&access);

    assert_eq!(
        text(&field(
            &field(&observation, "server"),
            "authorized_keys_command"
        )),
        "none"
    );
    let operator = field(&field(&observation, "accounts"), "operator");
    assert_eq!(items_of(&operator).len(), 1);
}

#[test]
fn read_accounts_names_the_passwd_file_it_could_not_read() {
    let passwd = tree("missing-passwd").join("passwd");
    let failure = files("missing-passwd", &passwd)
        .read_accounts(&server())
        .expect_err("a missing passwd file is a failure");

    assert!(failure.to_string().contains(&passwd.display().to_string()));
}

#[test]
fn read_accounts_refuses_a_passwd_line_with_the_wrong_number_of_columns() {
    let root = tree("bad-passwd-columns");
    let passwd = root.join("passwd");
    fs::write(&passwd, "operator:x:1000\n").expect("a writable passwd");

    let failure = files("bad-passwd-columns", &passwd)
        .read_accounts(&server())
        .expect_err("a malformed passwd line must fail");

    assert!(
        failure
            .to_string()
            .contains("expected 7 colon-separated columns")
    );
}

#[test]
fn read_accounts_skips_passwd_entries_missing_an_account_or_home() {
    let root = tree("blank-passwd-fields");
    let home = root.join("home/operator");
    fs::create_dir_all(home.join(".ssh")).expect("a writable home");
    fs::write(
        home.join(".ssh/authorized_keys"),
        format!("ssh-ed25519 {ED25519} operator\n"),
    )
    .expect("a writable file");
    let passwd = root.join("passwd");
    fs::write(
        &passwd,
        format!(
            ":x:1:1::{}:/bin/sh\nblankhome:x:2:2:::/bin/sh\noperator:x:1000:1000::{}:/bin/sh\n",
            home.display(),
            home.display()
        ),
    )
    .expect("a writable passwd");

    let accounts = files("blank-passwd-fields", &passwd)
        .read_accounts(&server())
        .expect("well formed");

    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].0, "operator");
}

#[test]
fn read_accounts_names_the_key_file_that_failed_to_parse() {
    let root = tree("bad-key-file");
    let home = root.join("home/operator");
    fs::create_dir_all(home.join(".ssh")).expect("a writable home");
    let key_file = home.join(".ssh/authorized_keys");
    fs::write(&key_file, "ssh-ed25519\n").expect("a writable key file");
    let passwd = root.join("passwd");
    fs::write(
        &passwd,
        format!("operator:x:1000:1000::{}:/bin/sh\n", home.display()),
    )
    .expect("a writable passwd");

    let failure = files("bad-key-file", &passwd)
        .read_accounts(&server())
        .expect_err("a malformed key file must fail");

    assert!(
        failure
            .to_string()
            .contains(&key_file.display().to_string())
    );
}

#[test]
fn read_accounts_ignores_a_path_whose_parent_is_not_a_directory() {
    let root = tree("not-a-directory");
    let home = root.join("home/operator");
    fs::create_dir_all(&home).expect("a writable home");
    fs::write(home.join(".ssh"), "not a directory").expect("a writable file");
    let passwd = root.join("passwd");
    fs::write(
        &passwd,
        format!("operator:x:1000:1000::{}:/bin/sh\n", home.display()),
    )
    .expect("a writable passwd");

    let accounts = files("not-a-directory", &passwd)
        .read_accounts(&server())
        .expect("a not-a-directory key path is treated as absent");

    assert!(accounts.is_empty());
}

#[test]
fn read_accounts_fails_when_a_configured_key_path_is_a_directory() {
    let root = tree("key-directory");
    let key_directory = root.join("keys");
    fs::create_dir_all(&key_directory).expect("a writable directory");
    let passwd = root.join("passwd");
    fs::write(&passwd, "operator:x:1000:1000::/home/operator:/bin/sh\n")
        .expect("a writable passwd");
    let mut configured = server();
    configured.authorized_keys_files = vec![key_directory.display().to_string()];

    let failure = files("key-directory", &passwd)
        .read_accounts(&configured)
        .expect_err("a directory is not a readable key file");

    assert!(
        failure
            .to_string()
            .contains(&key_directory.display().to_string())
    );
}

#[test]
fn a_restricted_key_says_so_without_the_reader_counting_a_list() {
    // Arrange: whether a key is restricted at all is the question an auditor asks first.
    let keys = authorized_keys::parse(&format!(
        "restrict,command=\"/usr/bin/backup\" ssh-ed25519 {ED25519} backup\nssh-ed25519 {ED25519} plain\n"
    ))
    .expect("well formed");
    let restricted = Observation::from(&keys[0]);
    let unrestricted = Observation::from(&keys[1]);

    // Assert
    assert_eq!(
        field(&restricted, "restricted").content(),
        &Content::Scalar(Scalar::Boolean(true))
    );
    assert_eq!(
        field(&unrestricted, "restricted").content(),
        &Content::Scalar(Scalar::Boolean(false))
    );
}

#[test]
fn the_key_body_is_recorded_and_is_not_marked_sensitive() {
    // Arrange: the deliberate contrast with the accounts facet, which records no password
    // hash. A public key grants nothing to whoever holds it, and it *is* the access grant, so
    // a key swapped under an unchanged comment would otherwise be invisible.
    let keys =
        authorized_keys::parse(&format!("ssh-ed25519 {ED25519} operator\n")).expect("well formed");

    // Act
    let observation = Observation::from(&keys[0]);

    // Assert
    assert_eq!(text(&field(&observation, "key")), ED25519);
    assert_eq!(
        field(&observation, "key").sensitivity(),
        rastro_fingerprint::Sensitivity::Public
    );
}

#[test]
fn the_facet_holds_the_server_settings_beside_the_accounts() {
    // Arrange: a hundred keys mean nothing when `PubkeyAuthentication` is `no`.
    let access = rastro::collectors::ssh_access::SshAccess::new(server(), Vec::new())
        .expect("a server with no keyed accounts");

    // Act
    let observation = Observation::from(&access);

    // Assert
    assert_eq!(
        object_of(&observation)
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<String>>(),
        ["accounts", "server"]
    );
    assert_eq!(
        text(&field(
            &field(&observation, "server"),
            "password_authentication"
        )),
        "no"
    );
}

#[test]
fn keys_from_two_files_are_gathered_under_one_account_and_sorted() {
    // Arrange: which file a key came from is not part of the grant.
    let first = authorized_keys::parse(&format!("ssh-rsa {ED25519} zulu\n")).expect("legal");
    let second = authorized_keys::parse(&format!("ssh-ed25519 {ED25519} alpha\n")).expect("legal");
    let access = rastro::collectors::ssh_access::SshAccess::new(
        server(),
        vec![
            ("operator".to_owned(), first),
            ("operator".to_owned(), second),
        ],
    )
    .expect("one account, two files");

    // Act
    let observation = Observation::from(&access);
    let keys = items_of(&field(&field(&observation, "accounts"), "operator"));

    // Assert
    assert_eq!(keys.len(), 2);
    assert_eq!(text(&field(&keys[0], "key_type")), "ssh-ed25519");
}

#[test]
fn presence_is_absent_without_an_sshd_rather_than_undetermined() {
    // Act & Assert: a box with no ssh server genuinely admits nobody over ssh, since the keys
    // in a home directory grant nothing with no daemon to honour them.
    assert_eq!(
        SshAccessCollector::reading(None).presence(),
        Presence::Absent
    );
}

#[test]
fn collect_fails_rather_than_reporting_no_access_without_an_sshd() {
    // Act & Assert
    assert!(SshAccessCollector::reading(None).collect().is_err());
}
