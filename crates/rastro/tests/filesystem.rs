//! The walker's policy table: which trees get their content read, which are known by
//! their metadata alone, and what a collector's claim over its own tree does to it.

mod support;

use rastro::collectors::filesystem::{ContentPolicy, DigestAlgorithm, PolicyRule, WalkPolicy};
use rastro_collector::{
    AbsolutePath, ClaimQualifier, FacetName, FilesystemClaim, Observation, WalkedTree,
};
use support::observation::{field, items_of, text};

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

fn qualifier(value: &str) -> ClaimQualifier {
    ClaimQualifier::new(value).expect("a legal qualifier")
}

/// The claimants an effective-table entry renders, which is a list however many there are.
fn texts_of(observation: &Observation) -> Vec<String> {
    items_of(observation).iter().map(text).collect()
}

fn claimants_of(rule: &PolicyRule) -> Vec<String> {
    rule.claimants
        .iter()
        .map(|claimant| claimant.to_string())
        .collect()
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
fn built_in_opens_no_file_anywhere() {
    // Arrange: hashing every regular file on every mount was measured on a production
    // PostgreSQL host at 84 GB read and climbing, 10.4M read syscalls, and a run killed
    // at 51 minutes having produced nothing. The walk stays total; what narrowed is the
    // reading, which is a cost question rather than a scope one.
    let policy = WalkPolicy::built_in();

    // Act & Assert: no tree is read, including the ones a config file would never have
    // thought to name. Detection survives on stat alone, because a write moves mtime and
    // ctime and ctime has no userspace setter at all.
    for stated in [
        "/etc/ssh/sshd_config",
        "/usr/bin/ls",
        "/usr/local/bin/node_exporter",
        "/root/.ssh/authorized_keys",
        "/srv/app/config.yml",
        "/home/dvidal/.bashrc",
        "/var/lib/postgresql/17/main/base/1/2337",
    ] {
        assert_eq!(
            policy.policy_for(&walked(stated)),
            &ContentPolicy::MetadataOnly,
            "{stated} is known by its metadata, not by its content"
        );
    }
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
    assert_eq!(claimants_of(rule), vec!["postgresql"]);
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

    // Assert: a claim narrows one tree and says nothing about any other, so `/usr` still
    // reports its metadata and the shipped churn list still churns.
    assert_eq!(
        policy.policy_for(&walked("/usr/bin/ls")),
        &ContentPolicy::MetadataOnly
    );
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
fn claimed_seals_a_tree_two_facets_claim() {
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
    let policy = claimed
        .claimed(&facet("mariadb"), &[FilesystemClaim::churns(contested)])
        .expect("a contested tree is sealed, not refused");

    // Assert: no winner is picked, and neither reading is honoured. Sealing is not rastro
    // settling the argument, it is rastro declining to walk into a tree it cannot account
    // for. Both claimants are kept, because that is what makes the argument fixable.
    let rule = rule_for(&policy, "/var/lib/mysql");
    assert_eq!(rule.content, ContentPolicy::Sealed);
    assert_eq!(claimants_of(rule), vec!["mariadb", "mysql"]);
}

#[test]
fn claimed_seals_a_tree_two_entries_of_one_facet_claim() {
    // Arrange: the host from issue #41. Two postgresql-common clusters registered on one data
    // directory, which is impossible as a running state and a real one as a registration.
    let shared = tree("/var/lib/postgresql/data");
    let claims = [
        FilesystemClaim::sealed(shared.clone()).for_entry(qualifier("11/main")),
        FilesystemClaim::sealed(shared).for_entry(qualifier("14/main")),
    ];

    // Act
    let policy = WalkPolicy::built_in()
        .claimed(&facet("postgresql"), &claims)
        .expect("a contested tree is sealed, not refused");

    // Assert: agreeing on the reading settles nothing. rastro cannot tell which cluster owns
    // the directory, and a cluster that was down while the walk ran may own it all the same,
    // so two claims on one tree is a fact rather than a duplicate to be tidied away.
    let rule = rule_for(&policy, "/var/lib/postgresql/data");
    assert_eq!(rule.content, ContentPolicy::Sealed);
    assert_eq!(
        claimants_of(rule),
        vec!["postgresql:11/main", "postgresql:14/main"]
    );
}

#[test]
fn claimed_seals_a_tree_a_shipped_rule_already_governs() {
    // Act
    let policy = WalkPolicy::built_in()
        .claimed(
            &facet("journald"),
            &[FilesystemClaim::churns(tree("/var/log"))],
        )
        .expect("a contested tree is sealed, not refused");

    // Assert: rastro's own shipped rule is a claimant like any other, so a collector that
    // duplicates one contests it. That is either a bug in rastro or a misconfigured box, and
    // both are worth the subtree until somebody fixes it.
    let rule = rule_for(&policy, "/var/log");
    assert_eq!(rule.content, ContentPolicy::Sealed);
    assert_eq!(claimants_of(rule), vec!["filesystem", "journald"]);
}

#[test]
fn claimed_seals_the_root_like_any_other_contested_tree() {
    // Arrange: nobody should claim the root, and the rule does not need to know that.
    let policy = WalkPolicy::built_in()
        .claimed(&facet("overlay"), &[FilesystemClaim::sealed(tree("/"))])
        .expect("a contested tree is sealed, not refused");

    // Assert: a special case here would buy a branch and nothing else. A sealed root leaves
    // one entry in the facet carrying both claimants, which says what happened; the carve-out
    // it would replace exists for a failure that could not say anything at all.
    let rule = rule_for(&policy, "/");
    assert_eq!(rule.content, ContentPolicy::Sealed);
    assert_eq!(claimants_of(rule), vec!["filesystem", "overlay"]);
}

#[test]
fn a_contested_tree_is_reported_as_contested() {
    // Arrange
    let contested = tree("/var/lib/mysql");
    let policy = WalkPolicy::built_in()
        .claimed(
            &facet("mysql"),
            &[FilesystemClaim::sealed(contested.clone())],
        )
        .expect("the first claim stands")
        .claimed(&facet("mariadb"), &[FilesystemClaim::sealed(contested)])
        .expect("a contested tree is sealed, not refused");

    // Act
    let contested: Vec<&str> = policy.contested().map(|rule| rule.tree.as_str()).collect();

    // Assert: one question, asked of the table rather than reconstructed by counting names,
    // because the walk reports every contested tree and must not miss one.
    assert_eq!(contested, vec!["/var/lib/mysql"]);
}

#[test]
fn a_contested_rule_says_why_the_tree_was_sealed() {
    // Arrange
    let contested = tree("/var/lib/postgresql/data");
    let policy = WalkPolicy::built_in()
        .claimed(
            &facet("postgresql"),
            &[FilesystemClaim::sealed(contested.clone()).for_entry(qualifier("11/main"))],
        )
        .expect("a tree no shipped rule names")
        .claimed(
            &facet("postgresql"),
            &[FilesystemClaim::sealed(contested).for_entry(qualifier("14/main"))],
        )
        .expect("a contested tree is sealed, not refused");

    // Act
    let said = rule_for(&policy, "/var/lib/postgresql/data")
        .contest()
        .expect("a contested rule says so");

    // Assert: one sentence, stated once, for the operator watching the run and for the reader
    // of the entry it cost. Two wordings of one fact is two things to keep true.
    assert_eq!(
        said,
        "claimed by postgresql:11/main, postgresql:14/main, so it was sealed rather than walked"
    );
}

#[test]
fn an_uncontested_rule_has_nothing_to_say() {
    // Act & Assert: every table has rules, and almost none of them are contested, so the
    // question answers absent rather than making every caller test the count itself.
    assert!(
        rule_for(&WalkPolicy::built_in(), "/var/log")
            .contest()
            .is_none()
    );
}

#[test]
fn an_operators_rule_settles_a_contested_tree() {
    // Arrange: a tree two collectors argued over, which rastro sealed.
    let contested = tree("/var/lib/mysql");
    let sealed = WalkPolicy::built_in()
        .claimed(
            &facet("mysql"),
            &[FilesystemClaim::sealed(contested.clone())],
        )
        .expect("the first claim stands")
        .claimed(
            &facet("mariadb"),
            &[FilesystemClaim::sealed(contested.clone())],
        )
        .expect("a contested tree is sealed, not refused");

    // Act
    let policy = sealed
        .configured(vec![PolicyRule::configured(
            contested,
            ContentPolicy::MetadataOnly,
        )])
        .expect("the operator's rule replaces what it names");

    // Assert: the escape hatch, and the reason sealing a tree is never a dead end. The
    // operator knows their box, so their rule replaces the seal outright and the tree stops
    // being contested rather than staying sealed with a note.
    let rule = rule_for(&policy, "/var/lib/mysql");
    assert_eq!(rule.content, ContentPolicy::MetadataOnly);
    assert_eq!(claimants_of(rule), vec!["config"]);
    assert_eq!(policy.contested().count(), 0);
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
    assert_eq!(texts_of(&field(&cluster, "claimed_by")), vec!["postgresql"]);

    let root = field(&rendered, "/");
    assert_eq!(text(&field(&root, "reading")), "metadata_only");
    assert_eq!(texts_of(&field(&root, "claimed_by")), vec!["filesystem"]);
}

#[test]
fn sha256_is_spelled_the_way_the_document_will_record_it() {
    // Act & Assert: the algorithm is named in the document rather than assumed,
    // because a sha256 fingerprint cannot be diffed against any other kind.
    assert_eq!(DigestAlgorithm::Sha256.as_str(), "sha256");
}

#[test]
fn an_operators_rule_beats_a_collectors_claim() {
    // Arrange: the postgresql collector seals its cluster, and the operator says they want its
    // metadata after all. A claim is rastro's guess about a tree from the outside; the operator
    // knows their box. So the operator wins, and the table says who decided.
    let cluster = tree("/var/lib/postgresql/17/main");
    let claimed = WalkPolicy::built_in()
        .claimed(&facet("postgresql"), &[FilesystemClaim::sealed(cluster)])
        .expect("a tree no shipped rule names");

    // Act
    let configured = claimed
        .configured(vec![PolicyRule::configured(
            tree("/var/lib/postgresql/17/main"),
            ContentPolicy::MetadataOnly,
        )])
        .expect("an operator's rule replaces a claim rather than contradicting it");

    // Assert
    let rule = rule_for(&configured, "/var/lib/postgresql/17/main");
    assert_eq!(rule.content, ContentPolicy::MetadataOnly);
    assert_eq!(claimants_of(rule), vec!["config"]);
}

#[test]
fn an_operators_rule_beats_a_shipped_one_too() {
    // Arrange: `/var/log` churns by rastro's own reckoning. An operator who seals it wants the
    // entries gone, not merely quiet.
    let policy = WalkPolicy::built_in()
        .configured(vec![PolicyRule::configured(
            tree("/var/log"),
            ContentPolicy::Sealed,
        )])
        .expect("an operator may override a shipped rule");

    // Act & Assert
    assert_eq!(
        policy.policy_for(&walked("/var/log/syslog")),
        &ContentPolicy::Sealed
    );
}

#[test]
fn an_operators_rules_leave_every_other_tree_alone() {
    // Arrange
    let policy = WalkPolicy::built_in()
        .configured(vec![PolicyRule::configured(
            tree("/srv/media"),
            ContentPolicy::Sealed,
        )])
        .expect("a tree no shipped rule names");

    // Act & Assert: a narrowing narrows one tree and says nothing about any other.
    assert_eq!(
        policy.policy_for(&walked("/usr/bin/ls")),
        &ContentPolicy::MetadataOnly
    );
    assert_eq!(
        policy.policy_for(&walked("/var/log/syslog")),
        &ContentPolicy::Churns
    );
}

#[test]
fn two_operator_rules_for_one_tree_are_refused() {
    // Act
    let refused = WalkPolicy::built_in().configured(vec![
        PolicyRule::configured(tree("/srv"), ContentPolicy::Sealed),
        PolicyRule::configured(tree("/srv"), ContentPolicy::Churns),
    ]);

    // Assert: naming one tree twice in one config has no most-specific answer, exactly as a
    // shipped table naming it twice does. The operator meant one of them and rastro cannot
    // know which.
    assert!(refused.is_err());
}

#[test]
fn the_effective_table_says_when_the_operator_decided() {
    // Arrange
    let policy = WalkPolicy::built_in()
        .configured(vec![PolicyRule::configured(
            tree("/home/runner/work"),
            ContentPolicy::Sealed,
        )])
        .expect("a tree no shipped rule names");

    // Act
    let rendered = Observation::from(&policy);

    // Assert: an operator's narrowing is a decision the document has to admit to, exactly as a
    // collector's claim is. Otherwise a reader of a tree with no entries cannot tell rastro's
    // reckoning from their own colleague's config.
    let sealed = field(&rendered, "/home/runner/work");
    assert_eq!(text(&field(&sealed, "reading")), "sealed");
    assert_eq!(texts_of(&field(&sealed, "claimed_by")), vec!["config"]);
}
