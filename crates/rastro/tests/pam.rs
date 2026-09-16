//! The `pam` facet: what PAM puts into a login session's environment.

mod support;

use rastro::collectors::pam::model::{EnvironmentRule, RulesFile, VariablesFile};
use rastro::collectors::pam::source::pam_env_conf;
use rastro::collectors::pam::{FileStatus, PamCollector, SessionEnvironment};
use rastro_collector::{
    AbsolutePath, Collector, EnvironmentVariableName, Observation, Presence, Presentation,
};
use rastro_fingerprint::{Content, Scalar};
use support::observation::{field, items_of, keys_of, text};

fn path(value: &str) -> AbsolutePath {
    AbsolutePath::new(value, "test path").expect("an absolute path")
}

fn name(value: &str) -> EnvironmentVariableName {
    EnvironmentVariableName::new(value).expect("a legal name")
}

fn rendered(environment: &SessionEnvironment) -> Observation {
    Observation::from(environment)
        .in_view(Presentation::complete())
        .expect("nothing here is volatile")
}

#[test]
fn presence_is_absent_when_the_box_does_not_use_pam() {
    // Arrange: `/etc/pam.d` is PAM's configuration root, so a box without it does not run
    // PAM and has no session environment for PAM to set. The same shape of answer `units`
    // gives for a box with no systemd.
    let collector = PamCollector::reading("/nonexistent/pam.d");

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Absent);
}

#[test]
fn presence_is_present_when_pam_configuration_is_on_the_box() {
    // Act & Assert: the crate's own directory stands in for a configuration root, because
    // what is being tested is that a directory answers present and a missing one does not.
    let collector = PamCollector::reading(env!("CARGO_MANIFEST_DIR"));

    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn a_rule_carries_its_default_and_its_override() {
    // Arrange: measured — `OVERRIDE` wins where both are present, which is why both are
    // recorded rather than one resolved value.
    let parsed = pam_env_conf::parse(
        "# a comment\nFROMCONF DEFAULT=fallback\nOVERRIDDEN DEFAULT=low OVERRIDE=high\n",
    );

    // Assert
    assert_eq!(parsed.rules.len(), 2);
    assert_eq!(parsed.rules[0].name.as_str(), "FROMCONF");
    assert_eq!(parsed.rules[0].default.as_deref(), Some("fallback"));
    assert_eq!(parsed.rules[0].override_value, None);
    assert_eq!(parsed.rules[1].default.as_deref(), Some("low"));
    assert_eq!(parsed.rules[1].override_value.as_deref(), Some("high"));
}

#[test]
fn a_rule_value_is_recorded_as_written_and_not_expanded() {
    // Arrange: measured — `EXPANDED DEFAULT=@{HOME}/x` reaches one account's session as
    // `/home/probe/x`. The expansion is per account at login, so recording a resolved value
    // would mean naming one account's answer as though it were everybody's.
    let parsed = pam_env_conf::parse("EXPANDED DEFAULT=@{HOME}/x\n");

    // Assert
    assert_eq!(parsed.rules[0].default.as_deref(), Some("@{HOME}/x"));
}

#[test]
fn a_rule_naming_only_a_variable_is_kept_and_a_nameless_line_is_counted() {
    // Arrange: a bare name is a rule that sets nothing, and recording it is how a reader
    // sees somebody wrote one. A line whose first word is not a legal name declares no
    // variable at all.
    let parsed = pam_env_conf::parse("BARE\n=NONAME DEFAULT=x\n");

    // Assert
    assert_eq!(parsed.rules.len(), 1);
    assert_eq!(parsed.rules[0].name.as_str(), "BARE");
    assert_eq!(parsed.ignored_lines, 1);
}

#[test]
fn the_facet_names_both_sources_and_withholds_only_the_values() {
    // Arrange
    let environment = SessionEnvironment {
        variables: VariablesFile {
            path: path("/etc/environment"),
            status: FileStatus::Ok,
            variables: [(name("HTTP_PROXY"), "http://proxy:3128".to_owned())]
                .into_iter()
                .collect(),
            ignored_lines: 1,
        },
        rules: RulesFile {
            path: path("/etc/security/pam_env.conf"),
            status: FileStatus::Ok,
            rules: vec![EnvironmentRule {
                name: name("LANG"),
                default: Some("en_GB.UTF-8".to_owned()),
                override_value: None,
            }],
            ignored_lines: 0,
        },
    };

    // Act
    let observed = rendered(&environment);
    let variables = field(&observed, "variables");
    let rules = field(&observed, "rules");

    // Assert: the name is the migration finding and is not a secret; the value is where a
    // proxy credential would be.
    assert_eq!(keys_of(&field(&variables, "variables")), ["HTTP_PROXY"]);
    assert!(
        text(&field(&field(&variables, "variables"), "HTTP_PROXY"))
            .starts_with("redacted:sha256+xxh3:")
    );
    let rule = &items_of(&field(&rules, "rules"))[0];
    assert_eq!(text(&field(rule, "name")), "LANG");
    assert!(text(&field(rule, "default")).starts_with("redacted:sha256+xxh3:"));
    assert_eq!(
        field(rule, "override").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn a_source_that_is_not_on_the_box_is_absent_and_carries_no_empty_map() {
    // Arrange: absence is state. An empty `variables` object would say the file is there and
    // sets nothing, which is a different fact from the file not being there.
    let environment = SessionEnvironment {
        variables: VariablesFile {
            path: path("/etc/environment"),
            status: FileStatus::Absent,
            variables: Default::default(),
            ignored_lines: 0,
        },
        rules: RulesFile {
            path: path("/etc/security/pam_env.conf"),
            status: FileStatus::Unreadable("Permission denied (os error 13)".to_owned()),
            rules: Vec::new(),
            ignored_lines: 0,
        },
    };

    // Act
    let observed = rendered(&environment);
    let variables = field(&observed, "variables");
    let rules = field(&observed, "rules");

    // Assert
    assert_eq!(text(&field(&variables, "status")), "absent");
    assert_eq!(
        field(&variables, "variables").content(),
        &Content::Scalar(Scalar::Null)
    );
    assert_eq!(
        field(&variables, "ignored_lines").content(),
        &Content::Scalar(Scalar::Null),
        "a count of zero would read as a file that parsed cleanly"
    );
    assert_eq!(text(&field(&rules, "status")), "error");
    assert!(text(&field(&rules, "error")).contains("Permission denied"));
}

#[test]
fn every_source_carries_the_same_keys_whatever_was_read() {
    // Arrange: the output format is the contract, and a key that appears only sometimes is
    // awkward for every consumer.
    let environment = SessionEnvironment {
        variables: VariablesFile {
            path: path("/etc/environment"),
            status: FileStatus::Absent,
            variables: Default::default(),
            ignored_lines: 0,
        },
        rules: RulesFile {
            path: path("/etc/security/pam_env.conf"),
            status: FileStatus::Ok,
            rules: Vec::new(),
            ignored_lines: 0,
        },
    };

    // Act
    let observed = rendered(&environment);

    // Assert
    assert_eq!(keys_of(&observed), ["rules", "variables"]);
    assert_eq!(
        keys_of(&field(&observed, "variables")),
        ["error", "ignored_lines", "path", "status", "variables"]
    );
    assert_eq!(
        keys_of(&field(&observed, "rules")),
        ["error", "ignored_lines", "path", "rules", "status"]
    );
}
