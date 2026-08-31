//! Reading the local account database, without needing an `/etc` to read it from.
//!
//! Every account in here is invented. The box this collector was developed against
//! carries real people's login names in its group lists, and a fixture is a file in
//! a public repository.

use std::fs;
use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::accounts::{
    AccountFiles, AccountRegistry, AccountsCollector, EtcGroup, EtcPasswd, PasswordStatus,
    ShadowEntry, UserName,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar};
use support::observation::{field, object_of};
/// Three accounts covering the shapes an ordinary box has: a privileged one with a
/// real shell, a system one with no comment and no login, and a human one.
const PASSWD: &str = "\
root:x:0:0:root:/root:/bin/bash
sysdaemon:x:101:106::/nonexistent:/usr/sbin/nologin
operator:x:1000:1000:The Operator,,,:/home/operator:/bin/sh
";

const GROUP: &str = "\
root:x:0:
adm:x:4:operator
sysdaemon:x:106:
operator:x:1000:
";

const SHADOW: &str = "\
root:*:20583:0:99999:7:::
sysdaemon:!:20583:0:99999:7:::
operator:$y$j9T$abcdefghijklmnop$0123456789:20672:0:99999:7:::
";

fn registry(passwd: &str, group: &str, shadow: Option<&str>) -> AccountRegistry {
    let root = tree_for(passwd, group, shadow);

    files_in(&root)
        .read()
        .expect("this account database is well formed")
}

fn files_in(root: &Path) -> AccountFiles {
    AccountFiles::at(root.join("passwd"), root.join("group"), root.join("shadow"))
}

/// A scratch `/etc` holding the three files, with the shadow file left out entirely
/// when the caller passes none.
fn tree_for(passwd: &str, group: &str, shadow: Option<&str>) -> PathBuf {
    // Unique per *test*, which under `cargo nextest` means unique per process too. The
    // counter alone was not: nextest gives each test its own process, so every one of them
    // started at zero, chose the same directory and deleted it out from under the others.
    // libtest hid that by running the whole binary in one process.
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let ordinal = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let process = std::process::id();

    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("accounts-{process}-{ordinal}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a writable scratch directory");

    fs::write(root.join("passwd"), passwd).expect("a writable passwd");
    fs::write(root.join("group"), group).expect("a writable group");
    if let Some(shadow) = shadow {
        fs::write(root.join("shadow"), shadow).expect("a writable shadow");
    }

    root
}

fn user(name: &str) -> UserName {
    UserName::new(name).expect("a legal user name")
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn integer(observation: &Observation) -> i64 {
    match observation.content() {
        Content::Scalar(Scalar::Integer(value)) => *value,
        other => panic!("expected an integer, got {other:?}"),
    }
}

fn list(observation: &Observation) -> Vec<String> {
    match observation.content() {
        Content::List(items) => items.iter().map(text).collect(),
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn parse_reads_every_column_of_a_passwd_line() {
    // Act
    let entries = EtcPasswd::parse(PASSWD).expect("this file is well formed");

    // Assert
    let operator = entries
        .iter()
        .find(|entry| entry.name.as_str() == "operator")
        .expect("the operator account");
    assert_eq!(operator.user_id.as_u32(), 1000);
    assert_eq!(operator.primary_group_id.as_u32(), 1000);
    assert_eq!(operator.comment.as_str(), "The Operator,,,");
    assert_eq!(
        operator
            .home_directory
            .as_ref()
            .map(|path| path.as_str().to_owned()),
        Some("/home/operator".to_owned())
    );
    assert_eq!(
        operator
            .login_shell
            .as_ref()
            .map(|path| path.as_str().to_owned()),
        Some("/bin/sh".to_owned())
    );
}

#[test]
fn parse_keeps_the_comment_field_whole_rather_than_splitting_its_commas() {
    // Act
    let entries = EtcPasswd::parse(PASSWD).expect("well formed");

    // Assert: the four comma-separated subfields are a `finger` convention, not a
    // rule of the file, and splitting would invent three empty fields here.
    let operator = entries
        .iter()
        .find(|entry| entry.name.as_str() == "operator")
        .expect("the operator account");
    assert_eq!(operator.comment.as_str(), "The Operator,,,");
}

#[test]
fn parse_accepts_an_empty_comment() {
    // Act
    let entries = EtcPasswd::parse(PASSWD).expect("well formed");

    // Assert: most system accounts have none, so blank is ordinary rather than a
    // misread.
    let daemon = entries
        .iter()
        .find(|entry| entry.name.as_str() == "sysdaemon")
        .expect("the system account");
    assert_eq!(daemon.comment.as_str(), "");
}

#[test]
fn parse_records_a_blank_shell_as_absent_rather_than_as_the_default() {
    // Arrange: the kernel substitutes `/bin/sh` at login, but the file says nothing.
    let passwd = "minimal:x:1:1::/home/minimal:\n";

    // Act
    let entries = EtcPasswd::parse(passwd).expect("well formed");

    // Assert: recording `/bin/sh` would put a value in the document that is not in
    // the file.
    assert_eq!(entries[0].login_shell, None);
}

#[test]
fn parse_refuses_a_passwd_line_with_the_wrong_number_of_columns() {
    // Act
    let result = EtcPasswd::parse("truncated:x:1:1:/home\n");

    // Assert
    let failure = result.expect_err("a truncated line must not be accepted");
    assert!(
        failure.to_string().contains("columns"),
        "the message must say what was wrong, got: {failure}"
    );
}

#[test]
fn parse_refuses_a_relative_home_directory() {
    // Act: a relative path in a positional file is the signal that the line was
    // tokenised into the wrong slots.
    let result = EtcPasswd::parse("odd:x:1:1::home/odd:/bin/sh\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_skips_a_commented_out_account() {
    // Arrange: glibc's own parser drops comments and blank lines before parsing, so
    // a commented account genuinely is not an account.
    let passwd = "# retired:x:1:1::/home/retired:/bin/sh\n\nreal:x:2:2::/home/real:/bin/sh\n";

    // Act
    let entries = EtcPasswd::parse(passwd).expect("well formed");

    // Assert
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name.as_str(), "real");
}

#[test]
fn parse_refuses_a_compat_directive_rather_than_under_reporting_the_accounts() {
    // Arrange: `+::::::` pulls in every account a directory server offers, so the
    // local file is a fragment of the database rather than the whole of it.
    let passwd = "root:x:0:0:root:/root:/bin/bash\n+::::::\n";

    // Act
    let result = EtcPasswd::parse(passwd);

    // Assert: refused, because reporting three local accounts as the answer to "who
    // can log in" would be the one failure this project will not accept.
    let failure = result.expect_err("a compat directive must not be silently skipped");
    assert!(
        failure.to_string().contains("directory service"),
        "the message must explain why the list is incomplete, got: {failure}"
    );
}

#[test]
fn parse_reads_a_groups_members() {
    // Act
    let groups = EtcGroup::parse(GROUP).expect("this file is well formed");

    // Assert
    let (_, adm) = groups
        .iter()
        .find(|(name, _)| name.as_str() == "adm")
        .expect("the adm group");
    assert_eq!(adm.group_id.as_u32(), 4);
    assert_eq!(
        adm.members
            .iter()
            .map(UserName::as_str)
            .collect::<Vec<&str>>(),
        ["operator"]
    );
}

#[test]
fn parse_reads_an_empty_member_column_as_no_members() {
    // Act
    let groups = EtcGroup::parse(GROUP).expect("well formed");

    // Assert: splitting `""` on a comma yields one empty name, which would fail on
    // the first of the hundred groups that have no members.
    let (_, root) = groups
        .iter()
        .find(|(name, _)| name.as_str() == "root")
        .expect("the root group");
    assert!(root.members.is_empty());
}

#[test]
fn parse_sorts_a_groups_members() {
    // Arrange: the file's order is the order names were appended in, which changes
    // when a user is removed and re-added without access having changed.
    let group = "wheel:x:10:carol,alice,bob\n";

    // Act
    let groups = EtcGroup::parse(group).expect("well formed");

    // Assert
    assert_eq!(
        groups[0]
            .1
            .members
            .iter()
            .map(UserName::as_str)
            .collect::<Vec<&str>>(),
        ["alice", "bob", "carol"]
    );
}

#[test]
fn parse_refuses_a_group_line_with_the_wrong_number_of_columns() {
    // Act & Assert
    assert!(EtcGroup::parse("wheel:x:10\n").is_err());
}

#[test]
fn password_status_reads_an_empty_field_as_no_password_at_all() {
    // Act: the most consequential value the column can hold, and the easiest to
    // miss in a file full of asterisks.
    let status = PasswordStatus::parse("");

    // Assert
    assert_eq!(status, PasswordStatus::Absent);
    assert_eq!(status.as_str(), "absent");
}

#[test]
fn password_status_keeps_the_placeholder_a_tool_wrote() {
    // Act: `adduser` leaves `*` where `useradd` leaves `!!`, and telling them apart
    // is how an operator knows which tool made the account.
    let star = PasswordStatus::parse("*");
    let bangs = PasswordStatus::parse("!!");

    // Assert
    assert_eq!(
        star,
        PasswordStatus::Unusable {
            marker: "*".to_owned()
        }
    );
    assert_eq!(
        bangs,
        PasswordStatus::Unusable {
            marker: "!!".to_owned()
        }
    );
}

#[test]
fn password_status_reads_a_hash_as_usable_and_names_the_scheme() {
    // Act
    let status = PasswordStatus::parse("$y$j9T$abcdefghijklmnop$0123456789");

    // Assert: the scheme is state worth diffing, since a release upgrade migrating
    // SHA-512 to yescrypt shows up here and nowhere else.
    match status {
        PasswordStatus::Usable { algorithm } => {
            assert_eq!(algorithm.expect("an algorithm").as_str(), "y");
        }
        other => panic!("expected a usable password, got {other:?}"),
    }
}

#[test]
fn password_status_tells_a_locked_hash_apart_from_a_placeholder() {
    // Act: `passwd -l` prefixes the hash, and `passwd -u` brings it back, so this is
    // not the same as never having had a password.
    let status = PasswordStatus::parse("!$6$salt$hash");

    // Assert
    match status {
        PasswordStatus::Locked { algorithm } => {
            assert_eq!(algorithm.expect("an algorithm").as_str(), "6");
        }
        other => panic!("expected a locked password, got {other:?}"),
    }
}

#[test]
fn password_status_reads_a_legacy_crypt_hash_as_usable_with_no_scheme() {
    // Act: thirteen characters and no `$` anywhere is a password set before 1999,
    // and somebody can still log in with it.
    let status = PasswordStatus::parse("ab1cdEfGh2iJk");

    // Assert: reporting this as an unusable placeholder would read as "nobody can
    // log in here" about an account where somebody can.
    assert_eq!(status, PasswordStatus::Usable { algorithm: None });
}

#[test]
fn password_status_never_keeps_the_hash() {
    // Act
    let status = PasswordStatus::parse("$y$j9T$SECRETSALT$SECRETHASH");
    let rendered = format!("{:?}", Observation::from(&status));

    // Assert: there is no field in the model a hash could be stored in, which is a
    // stronger guarantee than marking one sensitive while redaction is unbuilt.
    assert!(
        !rendered.contains("SECRET"),
        "no part of a hash may reach the document, got: {rendered}"
    );
}

#[test]
fn read_joins_a_user_to_its_shadow_entry() {
    // Act
    let registry = registry(PASSWD, GROUP, Some(SHADOW));

    // Assert
    let operator = registry
        .users()
        .get(&user("operator"))
        .expect("the operator");
    let aging = operator
        .aging
        .as_ref()
        .expect("the shadow database was read");
    assert_eq!(aging.last_changed_days_since_epoch, Some(20672));
    assert_eq!(aging.maximum_age_days, Some(99999));
    assert_eq!(aging.inactive_days, None);
    assert_eq!(aging.expires_days_since_epoch, None);
}

#[test]
fn read_leaves_the_password_unknown_when_the_host_keeps_no_shadow_database() {
    // Arrange: an old or minimal layout, which rastro can describe honestly.
    let registry = registry(PASSWD, GROUP, None);

    // Act
    let root = registry.users().get(&user("root")).expect("root");

    // Assert: absent, and emphatically not `PasswordStatus::Absent`, which would
    // claim the account needs no password.
    assert_eq!(root.password, None);
    assert_eq!(root.aging, None);
}

#[test]
fn read_refuses_a_user_the_shadow_database_does_not_mention() {
    // Arrange: `pwck` reports the same fault.
    let root = tree_for(PASSWD, GROUP, Some("root:*:20583:0:99999:7:::\n"));

    // Act
    let result = files_in(&root).read();

    // Assert: recording the password as absent would turn an inconsistency into a
    // claim that the account is wide open.
    let failure = result.expect_err("a user with no password record must not be guessed at");
    assert!(
        failure.to_string().contains("operator") || failure.to_string().contains("sysdaemon"),
        "the message must name the account, got: {failure}"
    );
}

#[test]
fn read_tolerates_a_shadow_line_naming_no_account() {
    // Arrange: a stale leftover. Every lookup starts from the passwd file, so this
    // line grants nothing to anybody.
    let shadow = format!("{SHADOW}retired:!:20000:0:99999:7:::\n");
    let root = tree_for(PASSWD, GROUP, Some(&shadow));

    // Act
    let registry = files_in(&root).read().expect("a stale line is not a fault");

    // Assert
    assert_eq!(registry.users().len(), 3);
}

#[test]
fn read_refuses_a_user_defined_twice() {
    // Arrange: `useradd` will not create this, but `vi` will, and every lookup
    // silently returns whichever line comes first.
    let passwd = format!("{PASSWD}operator:x:1001:1001::/home/other:/bin/sh\n");
    let root = tree_for(&passwd, GROUP, None);

    // Act
    let result = files_in(&root).read();

    // Assert
    let failure = result.expect_err("a duplicated account must not be silently dropped");
    assert!(
        failure.to_string().contains("operator"),
        "the message must name the account, got: {failure}"
    );
}

#[test]
fn read_refuses_a_group_defined_twice() {
    // Arrange
    let group = format!("{GROUP}adm:x:99:\n");
    let root = tree_for(PASSWD, &group, None);

    // Act & Assert
    assert!(files_in(&root).read().is_err());
}

#[test]
fn a_shadow_failure_never_quotes_the_line_it_came_from() {
    // Arrange: every other parser here quotes the offending line, because that is
    // what makes a failure actionable. This one must not, because the line is a
    // credential.
    let malformed = "operator:$y$j9T$SECRETSALT:20672:0\n";

    // Act
    let result = ShadowEntry::parse(malformed);

    // Assert
    let failure = result
        .expect_err("a truncated shadow line must fail")
        .to_string();
    assert!(
        !failure.contains("SECRET"),
        "a failure must not quote a credential, got: {failure}"
    );
    assert!(
        failure.contains("columns"),
        "the message must still say what was wrong, got: {failure}"
    );
}

#[test]
fn the_facet_holds_users_and_groups_side_by_side() {
    // Act
    let observation = Observation::from(&registry(PASSWD, GROUP, Some(SHADOW)));

    // Assert: neither file answers "who can do what" on its own.
    let keys: Vec<String> = object_of(&observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, ["groups", "users"]);
}

#[test]
fn a_user_renders_every_field_a_reader_needs() {
    // Act
    let observation = Observation::from(&registry(PASSWD, GROUP, Some(SHADOW)));
    let operator = field(&field(&observation, "users"), "operator");

    // Assert
    assert_eq!(integer(&field(&operator, "user_id")), 1000);
    assert_eq!(integer(&field(&operator, "primary_group_id")), 1000);
    assert_eq!(text(&field(&operator, "comment")), "The Operator,,,");
    assert_eq!(text(&field(&operator, "home_directory")), "/home/operator");
    assert_eq!(text(&field(&operator, "login_shell")), "/bin/sh");
    assert_eq!(
        text(&field(&field(&operator, "password"), "state")),
        "usable"
    );
    assert_eq!(
        text(&field(&field(&operator, "password"), "algorithm")),
        "y"
    );
}

#[test]
fn a_group_renders_its_members() {
    // Act
    let observation = Observation::from(&registry(PASSWD, GROUP, Some(SHADOW)));
    let adm = field(&field(&observation, "groups"), "adm");

    // Assert
    assert_eq!(integer(&field(&adm, "group_id")), 4);
    assert_eq!(list(&field(&adm, "members")), ["operator"]);
}

#[test]
fn presence_is_present_when_the_host_keeps_a_local_account_database() {
    // Arrange
    let root = tree_for(PASSWD, GROUP, Some(SHADOW));

    // Act & Assert
    assert_eq!(
        AccountsCollector::reading(files_in(&root)).presence(),
        Presence::Present
    );
}

#[test]
fn presence_is_absent_when_there_is_no_passwd_file() {
    // Arrange: an image built from scratch really does keep no local accounts, and
    // that is state rather than a failure.
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("accounts-none");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a writable scratch directory");

    // Act & Assert
    assert_eq!(
        AccountsCollector::reading(files_in(&root)).presence(),
        Presence::Absent
    );
}

#[test]
fn collect_reports_the_whole_database() {
    // Arrange
    let root = tree_for(PASSWD, GROUP, Some(SHADOW));

    // Act
    let collected = AccountsCollector::reading(files_in(&root))
        .collect()
        .expect("this database is well formed");

    // Assert
    assert_eq!(object_of(&field(&collected, "users")).len(), 3);
    assert_eq!(object_of(&field(&collected, "groups")).len(), 4);
}
