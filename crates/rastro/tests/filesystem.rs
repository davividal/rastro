//! The walker's policy table: which trees get their content read, and which are
//! known by their metadata alone.

use rastro::collectors::filesystem::{
    ContentPolicy, DigestAlgorithm, PolicyRule, WalkPolicy, WalkedTree,
};
use rastro_collector::AbsolutePath;

fn tree(value: &str) -> WalkedTree {
    WalkedTree::new(value).expect("a legal tree")
}

fn walked(value: &str) -> AbsolutePath {
    AbsolutePath::new(value, "walked path").expect("a legal path")
}

fn hashed() -> ContentPolicy {
    ContentPolicy::Hashed(DigestAlgorithm::Sha256)
}

fn rule(value: &str, content: ContentPolicy) -> PolicyRule {
    PolicyRule {
        tree: tree(value),
        content,
    }
}

fn table(rules: Vec<PolicyRule>) -> WalkPolicy {
    WalkPolicy::new(rules).expect("a legal table")
}

#[test]
fn policy_for_falls_back_to_the_root_rule() {
    // Arrange: the root rule is data rather than an implicit else, which is why
    // `policy_for` can answer for every path without an optional in its signature.
    let policy = table(vec![rule("/", hashed())]);

    // Act & Assert
    assert_eq!(policy.policy_for(&walked("/usr/bin/ls")), &hashed());
}

#[test]
fn policy_for_prefers_the_most_specific_tree() {
    // Arrange
    let policy = table(vec![
        rule("/", hashed()),
        rule("/var", hashed()),
        rule("/var/log", ContentPolicy::MetadataOnly),
    ]);

    // Act & Assert
    assert_eq!(
        policy.policy_for(&walked("/var/log/journal/system.journal")),
        &ContentPolicy::MetadataOnly
    );
}

#[test]
fn policy_for_matches_whole_components_rather_than_text_prefixes() {
    // Arrange: `/var/logrotate.conf` is not inside `/var/log`, and a `starts_with`
    // over the text would say it was. Getting this wrong silently stops hashing a
    // neighbouring tree, which is the kind of gap a fingerprint cannot report.
    let policy = table(vec![
        rule("/", hashed()),
        rule("/var/log", ContentPolicy::MetadataOnly),
    ]);

    // Act & Assert
    assert_eq!(policy.policy_for(&walked("/var/logrotate.conf")), &hashed());
    assert_eq!(policy.policy_for(&walked("/var/logging")), &hashed());
}

#[test]
fn policy_for_covers_the_tree_named_by_the_rule_itself() {
    // Arrange
    let policy = table(vec![
        rule("/", hashed()),
        rule("/var/log", ContentPolicy::MetadataOnly),
    ]);

    // Act & Assert: the directory a rule names is inside that rule, not merely
    // everything beneath it.
    assert_eq!(
        policy.policy_for(&walked("/var/log")),
        &ContentPolicy::MetadataOnly
    );
}

#[test]
fn new_refuses_two_rules_for_one_tree() {
    // Arrange
    let contradiction = vec![
        rule("/", hashed()),
        rule("/var/log", hashed()),
        rule("/var/log", ContentPolicy::MetadataOnly),
    ];

    // Act
    let refused = WalkPolicy::new(contradiction);

    // Assert: two policies for one tree has no most-specific answer, so the table
    // is rejected where it is built rather than resolved by rule order.
    assert!(refused.is_err());
}

#[test]
fn new_refuses_a_table_that_does_not_cover_the_root() {
    // Act
    let refused = WalkPolicy::new(vec![rule("/var/log", ContentPolicy::MetadataOnly)]);

    // Assert
    assert!(refused.is_err());
}

#[test]
fn new_reads_a_tree_spelled_with_a_trailing_slash() {
    // Arrange: `/var/log/` and `/var/log` are the same tree, and a table that
    // matched neither would be a config that silently did nothing.
    let policy = table(vec![
        rule("/", hashed()),
        rule("/var/log/", ContentPolicy::MetadataOnly),
    ]);

    // Act & Assert
    assert_eq!(
        policy.policy_for(&walked("/var/log/syslog")),
        &ContentPolicy::MetadataOnly
    );
}

#[test]
fn new_reads_the_root_however_many_separators_it_is_spelled_with() {
    // Arrange: every separator of `///` is trailing, so trimming leaves nothing. Kept
    // as spelled it is a tree of depth zero that no walked path matches, which turns
    // the constructor's promise of an answer for every path into a panic.
    let policy = table(vec![rule("///", hashed())]);

    // Act & Assert
    assert_eq!(policy.policy_for(&walked("/usr/bin/ls")), &hashed());
}

#[test]
fn new_refuses_the_root_named_twice_under_two_spellings() {
    // Act
    let refused = WalkPolicy::new(vec![
        rule("/", hashed()),
        rule("//", ContentPolicy::MetadataOnly),
    ]);

    // Assert: the refusal is about the tree, not the text, so a second spelling of one
    // the table already names is the same contradiction as repeating it verbatim.
    assert!(refused.is_err());
}

#[test]
fn built_in_hashes_what_it_was_not_told_to_leave_alone() {
    // Act
    let policy = WalkPolicy::built_in();

    // Assert: hashing is the default because config can only narrow. A table that
    // had to name the trees worth hashing would be an inclusion list.
    assert_eq!(
        policy.policy_for(&walked("/etc/ssh/sshd_config")),
        &hashed()
    );
    assert_eq!(policy.policy_for(&walked("/usr/bin/ls")), &hashed());
    assert_eq!(
        policy.policy_for(&walked("/usr/local/bin/node_exporter")),
        &hashed()
    );
}

#[test]
fn built_in_downgrades_the_trees_that_churn_without_meaning() {
    // Arrange
    let policy = WalkPolicy::built_in();

    // Act & Assert: measured on an idle Debian 12, where the only entries that moved
    // in ninety seconds were two journals and the timesync clock.
    for churning in [
        "/tmp/scratch",
        "/var/tmp/staged",
        "/var/log/syslog",
        "/var/log/journal/system.journal",
        "/var/cache/apt/pkgcache.bin",
        "/var/lib/systemd/timesync/clock",
        "/var/lib/systemd/random-seed",
    ] {
        assert_eq!(
            policy.policy_for(&walked(churning)),
            &ContentPolicy::MetadataOnly,
            "{churning} changes without meaning changing"
        );
    }
}

#[test]
fn sha256_is_spelled_the_way_the_document_will_record_it() {
    // Act & Assert: the algorithm is named in the document rather than assumed,
    // because a sha256 fingerprint cannot be diffed against any other kind.
    assert_eq!(DigestAlgorithm::Sha256.as_str(), "sha256");
}
