//! Which collectors a config leaves running.

use rastro::collectors;
use rastro::collectors::filesystem::Detail;
use rastro::config::Config;
use rastro_fingerprint::{Observation, View};

fn effective(config: &Config) -> Observation {
    collectors::effective_config(config, View::Diffable, false, Detail::Summary)
}

/// A run resolved as the composition root would, with the two host readings supplied so this
/// test needs neither a clock nor a `/proc`.
fn run(effective_config: Observation) -> collectors::Run {
    collectors::Run {
        effective_config,
        staged_binary: false,
        detail: Detail::Summary,
        started_at: Ok(1_786_632_455),
        hostname: Ok("reference-box".to_owned()),
        output: None,
        progress: None,
        narrowed: collectors::Narrowed::default(),
    }
}

fn names(config: &str) -> Vec<String> {
    let config = Config::parse(config).expect("this config is well formed");
    collectors::selected(collectors::built_in(run(effective(&config))), &config)
        .expect("this config is acceptable")
        .running()
        .iter()
        .map(|collector| collector.name().as_str().to_owned())
        .collect()
}

#[test]
fn every_collector_runs_when_nothing_is_excluded() {
    // Act
    let running = names("");

    // Assert: the default has to be everything, because the premise is a box
    // nobody documented.
    assert_eq!(
        running.len(),
        collectors::built_in(run(Observation::null())).len()
    );
}

#[test]
fn an_excluded_collector_does_not_run() {
    // Act
    let running = names("[collectors]\nexclude = [\"mounts\"]\n");

    // Assert
    assert!(!running.contains(&"mounts".to_owned()));
    assert_eq!(
        running.len(),
        collectors::built_in(run(Observation::null())).len() - 1
    );
}

#[test]
fn selected_reports_what_it_excluded_so_the_operator_can_be_told() {
    // Arrange
    let config = Config::parse("[collectors]\nexclude = [\"mounts\"]\n").expect("well formed");

    // Act
    let selection = collectors::selected(collectors::built_in(run(effective(&config))), &config)
        .expect("acceptable");

    // Assert: omitted from the document entirely, so the only trace is the
    // warning and the effective config in the envelope.
    assert_eq!(selection.excluded(), ["mounts"]);
}

#[test]
fn selected_refuses_a_collector_name_that_does_not_exist() {
    // Arrange: a typo would otherwise leave `mounts` running while the operator
    // believes it was switched off.
    let config = Config::parse("[collectors]\nexclude = [\"mount\"]\n").expect("well formed");

    // Act
    let result = collectors::selected(collectors::built_in(run(effective(&config))), &config);

    // Assert
    let failure = result.expect_err("an unknown collector must not be ignored");
    let message = failure.to_string();
    assert!(message.contains("mount"), "must name the typo: {message}");
    assert!(
        message.contains("mounts"),
        "must list what is available, so the typo is obvious: {message}"
    );
}

#[test]
fn selected_refuses_to_exclude_a_metadata_collector() {
    // Arrange
    let config = Config::parse("[collectors]\nexclude = [\"invocation\"]\n").expect("well formed");

    // Act
    let result = collectors::selected(collectors::built_in(run(effective(&config))), &config);

    // Assert: without it a fingerprint cannot be told apart from another, so
    // asking is a config mistake rather than something to quietly ignore.
    let failure = result.expect_err("metadata collectors cannot be switched off");
    assert!(failure.to_string().contains("invocation"));
}

#[test]
fn the_host_collector_cannot_be_excluded_either() {
    // Arrange
    let config = Config::parse("[collectors]\nexclude = [\"host\"]\n").expect("well formed");

    // Act & Assert
    assert!(collectors::selected(collectors::built_in(run(effective(&config))), &config).is_err());
}
