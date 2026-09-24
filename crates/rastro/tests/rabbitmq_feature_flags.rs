//! Which feature flags a node has enabled.
//!
//! Worth a third read of the node because enabling one is irreversible: it decides which
//! versions the node can be upgraded to and which nodes it can be clustered with, and none of
//! that is visible in the package version or the configuration.

use rastro::collectors::rabbitmq::RabbitmqctlFeatureFlags;

/// `rabbitmqctl list_feature_flags --formatter json` from the measured 4.0.5 node, trimmed to
/// four of its twenty-three rows: three it ships enabled and the one it does not.
const MEASURED: &str = r#"[
  {"name":"classic_mirrored_queue_version","state":"enabled"},
  {"name":"direct_exchange_routing_v2","state":"enabled"},
  {"name":"feature_flags_v2","state":"enabled"},
  {"name":"khepri_db","state":"disabled"}
]"#;

#[test]
fn parse_reads_a_flag_and_the_state_it_is_in() {
    // Act
    let flags = RabbitmqctlFeatureFlags::parse(MEASURED).expect("well formed");

    // Assert
    assert_eq!(flags.len(), 4);
    assert_eq!(flags.get("khepri_db").map(String::as_str), Some("disabled"));
    assert_eq!(
        flags.get("feature_flags_v2").map(String::as_str),
        Some("enabled")
    );
}

#[test]
fn parse_reads_past_a_runtime_report_here_too() {
    // Arrange: the same noise that can precede any of this tool's answers, and this one is an
    // array rather than an object.
    let noisy = format!(
        "=ERROR REPORT==== 23-Sep-2026::14:03:05 ===\nfile:path_eval([]): permission denied\n\n{MEASURED}"
    );

    // Act & Assert
    assert!(RabbitmqctlFeatureFlags::parse(&noisy).is_ok());
}

#[test]
fn parse_refuses_an_answer_that_is_not_a_list_of_flags() {
    // Act & Assert
    assert!(RabbitmqctlFeatureFlags::parse("Usage\n\nrabbitmqctl").is_err());
}

#[test]
fn parse_drops_a_flag_the_node_did_not_name() {
    // Act & Assert: a nameless row can only come from a RabbitMQ that changed the document,
    // and one unnameable flag is not worth the other twenty-two.
    let nameless = r#"[{"state":"enabled"},{"name":"khepri_db","state":"disabled"}]"#;
    let flags = RabbitmqctlFeatureFlags::parse(nameless).expect("a readable document");

    assert_eq!(flags.len(), 1);
}
