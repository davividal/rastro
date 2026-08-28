//! The walker's policy table: which trees get their content read, which are known by
//! their metadata alone, and what a collector's claim over its own tree does to it.

mod support;

use rastro::collectors::filesystem::{ContentPolicy, DigestAlgorithm, PolicyRule, WalkPolicy};
use rastro_collector::{AbsolutePath, FacetName, FilesystemClaim, Observation, WalkedTree};
use support::observation::{field, text};

fn facet(name: &str) -> FacetName {
    FacetName::new(name).expect("a legal facet name")
}

/// The rule a table holds for exactly this tree, rather than the one resolved for a path.
fn rule_for<'a>(policy: &'a WalkPolicy, tree: &str) -> &'a PolicyRule {
    policy
        .rules()
        .iter()
        .find(|rule| rule.tree.as_str() == tree)
        .unwrap_or_else(|| panic!("expected a rule for {tree:?}"))
}

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
    PolicyRule::shipped(tree(value), content)
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
fn built_in_marks_the_trees_that_churn_without_meaning_as_churning() {
    // Arrange
    let policy = WalkPolicy::built_in();

    // Act & Assert: measured on an idle Debian 12, where the only entries that moved
    // in ninety seconds were two journals and the timesync clock. `Churns` rather than
    // `MetadataOnly`, because withholding only the digest left those same journals in the
    // diff of a real cycle on their mtime alone.
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
            &ContentPolicy::Churns,
            "{churning} changes without meaning changing"
        );
    }
}

#[test]
fn claimed_reads_a_claimed_tree_the_way_its_claimant_asked() {
    // Arrange
    let cluster = tree("/var/lib/postgresql/17/main");

    // Act
    let policy = WalkPolicy::built_in()
        .claimed(&facet("postgresql"), &[FilesystemClaim::sealed(cluster)])
        .expect("a tree no shipped rule names");

    // Assert: the claim decides the reading, and the table records who asked, because a
    // reader of a tree with no entries under it is owed the name of whoever removed them.
    let rule = rule_for(&policy, "/var/lib/postgresql/17/main");
    assert_eq!(rule.content, ContentPolicy::Sealed);
    assert_eq!(rule.claimant.as_str(), "postgresql");
}

#[test]
fn claimed_reads_a_metadata_only_claim_as_metadata_only() {
    // Arrange: the level no built-in claimant uses yet, for a tree too large to hash whose
    // stamps are still the signal: a media store, where a new file arriving is exactly what
    // an operator wants to see.
    let store = tree("/srv/media");

    // Act
    let policy = WalkPolicy::built_in()
        .claimed(&facet("media"), &[FilesystemClaim::metadata_only(store)])
        .expect("a tree no shipped rule names");

    // Assert: content unread, and nothing else stepped back. That is the difference from
    // `Churns`, where the size, the inode and both stamps go volatile as well.
    let rule = rule_for(&policy, "/srv/media");
    assert_eq!(rule.content, ContentPolicy::MetadataOnly);
    assert!(!rule.content.churns());
    assert!(rule.content.is_descended());
}

#[test]
fn claimed_carries_the_shipped_rules_through_untouched() {
    // Arrange
    let claims = [FilesystemClaim::churns(tree("/var/lib/dpkg"))];

    // Act
    let policy = WalkPolicy::built_in()
        .claimed(&facet("packages"), &claims)
        .expect("a tree no shipped rule names");

    // Assert: a claim narrows one tree and says nothing about any other, so `/usr` is still
    // hashed and the shipped churn list still churns.
    assert_eq!(policy.policy_for(&walked("/usr/bin/ls")), &hashed());
    assert_eq!(
        policy.policy_for(&walked("/var/log/syslog")),
        &ContentPolicy::Churns
    );
    assert_eq!(
        policy.policy_for(&walked("/var/lib/dpkg/status")),
        &ContentPolicy::Churns
    );
}

#[test]
fn claimed_refuses_a_tree_another_rule_already_governs() {
    // Arrange: two collectors claiming one tree is a bug in a collector pair, and the box
    // that produces it is real: a MySQL and a MariaDB collector both naming the same data
    // directory because neither resolved it from the host.
    let contested = tree("/var/lib/mysql");
    let claimed = WalkPolicy::built_in()
        .claimed(
            &facet("mysql"),
            &[FilesystemClaim::sealed(contested.clone())],
        )
        .expect("the first claim stands");

    // Act
    let refused = claimed.claimed(&facet("mariadb"), &[FilesystemClaim::sealed(contested)]);

    // Assert: no winner is picked. The message names the tree and both claimants, because
    // that is what makes it fixable, and the walk fails rather than reading a table with no
    // most specific answer.
    let message = refused.expect_err("one tree, two rules").to_string();
    assert!(message.contains("/var/lib/mysql"), "got {message}");
    assert!(message.contains("mariadb"), "got {message}");
    assert!(message.contains("mysql"), "got {message}");
}

#[test]
fn claimed_refuses_a_claim_that_repeats_a_shipped_rule() {
    // Act
    let refused = WalkPolicy::built_in().claimed(
        &facet("journald"),
        &[FilesystemClaim::churns(tree("/var/log"))],
    );

    // Assert: agreeing by accident is not agreement. Accepting the duplicate would leave the
    // table silently ambiguous the moment either side moved.
    assert!(refused.is_err());
}

#[test]
fn the_effective_table_renders_every_rule_with_its_claimant() {
    // Arrange
    let policy = WalkPolicy::built_in()
        .claimed(
            &facet("postgresql"),
            &[FilesystemClaim::sealed(tree("/var/lib/postgresql/17/main"))],
        )
        .expect("a tree no shipped rule names");

    // Act
    let rendered = Observation::from(&policy);

    // Assert: this is the legend for every absent digest in the document, so it carries the
    // tree as its key, the reading, and the facet that asked for it.
    let cluster = field(&rendered, "/var/lib/postgresql/17/main");
    assert_eq!(text(&field(&cluster, "reading")), "sealed");
    assert_eq!(text(&field(&cluster, "claimed_by")), "postgresql");

    let root = field(&rendered, "/");
    assert_eq!(text(&field(&root, "reading")), "hashed");
    assert_eq!(text(&field(&root, "claimed_by")), "filesystem");
}

#[test]
fn sha256_is_spelled_the_way_the_document_will_record_it() {
    // Act & Assert: the algorithm is named in the document rather than assumed,
    // because a sha256 fingerprint cannot be diffed against any other kind.
    assert_eq!(DigestAlgorithm::Sha256.as_str(), "sha256");
}
