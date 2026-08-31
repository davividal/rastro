//! Which collectors a config leaves running.

use rastro::collectors;
use rastro::collectors::filesystem::Detail;
use rastro::config::Config;
use rastro_fingerprint::{Observation, View};

mod support;

use support::observation::{field, is_null, items_of, keys_of, text};

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

/// The collector of this name, from a run built as the composition root builds it.
fn collector_of(
    name: &str,
    narrowed: collectors::Narrowed,
) -> Box<dyn rastro_collector::Collector> {
    let config = Config::parse("").expect("this config is well formed");
    let mut resolved = run(effective(&config));
    resolved.narrowed = narrowed;

    collectors::built_in(resolved)
        .into_iter()
        .find(|collector| collector.name().as_str() == name)
        .expect("a built-in collector of this name")
}

#[test]
fn a_config_naming_something_that_is_not_a_tree_fails_only_the_walk() {
    // Arrange: an operator typo. A relative path cannot name a tree, because a walk rule has to
    // answer for a path and `narrow/this` does not say where.
    let narrowed = collectors::Narrowed {
        sealed: vec!["narrow/this".to_owned()],
        ..collectors::Narrowed::default()
    };

    // Act
    let refused = collector_of("filesystem", narrowed)
        .collect()
        .expect_err("a config that names no tree");

    // Assert: the message quotes what they wrote, because a config can hold several paths and
    // "not a tree" without one is a hunt.
    let message = refused.to_string();
    assert!(message.contains("narrow/this"), "got {message}");
    assert!(message.contains("is not one"), "got {message}");
}

#[test]
fn a_walk_table_that_could_not_be_resolved_is_null_rather_than_half_a_table() {
    // Arrange: the same typo. The `invocation` facet declares the effective walk table, and a
    // table that failed to resolve has no effective form — rendering the part that did resolve
    // would describe a walk that never happened.
    let narrowed = collectors::Narrowed {
        churns: vec!["../up-a-level".to_owned()],
        ..collectors::Narrowed::default()
    };

    // Act
    let declared = collector_of("invocation", narrowed)
        .collect()
        .expect("the invocation facet still describes the run");

    // Assert: absent, and loudly so. The `filesystem` facet carries the reason; this one must not
    // imply a table was in force.
    let table = field(&declared, "walk_policy");
    assert!(is_null(&table), "got {table:?}");
}

#[test]
fn a_selection_names_its_collectors_when_a_test_prints_it() {
    // Arrange: `Debug` is written by hand here rather than derived, and rather than putting the
    // bound on the `Collector` trait — a collector author should not have to derive anything to
    // satisfy an assertion in this crate. It only ever runs inside a failure message, which is
    // exactly why nothing else would notice it going wrong.
    let config = Config::parse("[collectors]\nexclude = [\"timers\"]\n").expect("well formed");
    let selection = collectors::selected(collectors::built_in(run(effective(&config))), &config)
        .expect("this config is acceptable");

    // Act
    let printed = format!("{selection:?}");

    // Assert: names, because names are what a failure needs.
    assert!(printed.contains("Selection"), "got {printed}");
    assert!(printed.contains("filesystem"), "got {printed}");
    assert!(printed.contains("timers"), "got {printed}");
}

#[test]
fn a_default_config_excludes_nothing_and_records_that_it_came_from_nowhere() {
    // Arrange: what a run with no `--config` resolves to. The premise is a box nobody
    // documented, so the default cannot ask the operator which collectors they want.
    //
    // Stated here rather than through the binary, and that is the point: a bare end-to-end run
    // walks every mount on the machine, so asserting these four facts that way cost three
    // minutes of CI and got slower on a bigger runner. None of them is about a walked path.
    let default = effective(&Config::default());

    // Assert
    assert!(
        items_of(&field(&default, "excluded_collectors")).is_empty(),
        "got {default:?}"
    );
    assert_eq!(text(&field(&default, "view")), "diffable");
    assert!(
        is_null(&field(&default, "source")),
        "no file was read, and a path here would name one that does not exist"
    );
}

#[test]
fn a_default_config_leaves_the_walk_total() {
    // Arrange: the run a bare invocation resolves to. What "no config collects everything" means
    // for the *walk* is that nothing narrowed it — the root is still read for metadata and no
    // rule in the effective table came from an operator.
    //
    // Asserted on the table rather than by watching a traversal, because the table is the
    // decision and the traversal is its consequence. Watching the consequence means walking every
    // mount on the machine, which is what made this cost three minutes of CI, and it would still
    // not have proved the root was unsealed — only that something was walked. There is no mock
    // for the walker from outside the process either: `FileTree::at` is the seam, and it is what
    // `filesystem_walk.rs` drives directly.
    let declared = collector_of("invocation", collectors::Narrowed::default())
        .collect()
        .expect("the invocation facet describes the run");

    // Act
    let table = field(&declared, "walk_policy");

    // Assert: the root is walked, and rastro is the one that decided so.
    let root = field(&table, "/");
    assert_eq!(text(&field(&root, "reading")), "metadata_only");
    assert_eq!(text(&field(&root, "claimed_by")), "filesystem");

    // Assert: and no tree in the table was narrowed by a config. This is what a seal in a test
    // config would have destroyed rather than demonstrated.
    for tree in keys_of(&table) {
        let rule = field(&table, &tree);
        assert_ne!(
            text(&field(&rule, "claimed_by")),
            "config",
            "{tree:?} was narrowed with no config to narrow it"
        );
    }
}
