use rastro_fingerprint::View;
use rastro_fingerprint::{Content, Observation, Sensitivity, Volatility};

fn entries_of(observation: &Observation) -> &std::collections::BTreeMap<String, Observation> {
    match observation.content() {
        Content::Object(entries) => entries,
        other => panic!("expected an object observation, got {other:?}"),
    }
}

fn items_of(observation: &Observation) -> &[Observation] {
    match observation.content() {
        Content::List(items) => items,
        other => panic!("expected a list observation, got {other:?}"),
    }
}

#[test]
fn a_new_observation_is_stable_and_public() {
    // Act
    let observation = Observation::text("nginx");

    // Assert
    assert_eq!(observation.volatility(), Volatility::Stable);
    assert_eq!(observation.sensitivity(), Sensitivity::Public);
}

#[test]
fn volatile_annotates_the_observation_without_changing_its_content() {
    // Arrange
    let plain = Observation::integer(612);

    // Act
    let annotated = plain.clone().volatile();

    // Assert
    assert_eq!(annotated.volatility(), Volatility::Volatile);
    assert_eq!(annotated.content(), plain.content());
}

#[test]
fn sensitive_annotates_the_observation_without_changing_its_content() {
    // Arrange
    let plain = Observation::text("hunter2");

    // Act
    let annotated = plain.clone().sensitive();

    // Assert
    assert_eq!(annotated.sensitivity(), Sensitivity::Sensitive);
    assert_eq!(annotated.content(), plain.content());
}

#[test]
fn an_observation_can_be_both_volatile_and_sensitive() {
    // Act
    let observation = Observation::text("session-token").volatile().sensitive();

    // Assert
    assert_eq!(observation.volatility(), Volatility::Volatile);
    assert_eq!(observation.sensitivity(), Sensitivity::Sensitive);
}

#[test]
fn object_orders_its_keys_regardless_of_insertion_order() {
    // Act
    let observation = Observation::object([
        ("zulu", Observation::integer(1)),
        ("mike", Observation::integer(2)),
        ("alpha", Observation::integer(3)),
    ]);

    // Assert
    let keys: Vec<&str> = entries_of(&observation)
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["alpha", "mike", "zulu"]);
}

#[test]
fn a_nested_observation_keeps_its_own_annotation() {
    // Act
    let observation = Observation::object([
        ("command", Observation::text("nginx")),
        ("pid", Observation::integer(991).volatile()),
    ]);

    // Assert
    let entries = entries_of(&observation);
    assert_eq!(entries["command"].volatility(), Volatility::Stable);
    assert_eq!(entries["pid"].volatility(), Volatility::Volatile);
    assert_eq!(
        observation.volatility(),
        Volatility::Stable,
        "annotating a child must not annotate its parent"
    );
}

#[test]
fn the_complete_view_keeps_a_volatile_value() {
    // Arrange
    let observation = Observation::object([
        ("command", Observation::text("nginx")),
        ("pid", Observation::integer(991).volatile()),
    ]);

    // Act
    let visible = observation
        .in_view(View::Complete)
        .expect("the complete view drops nothing");

    // Assert
    assert!(entries_of(&visible).contains_key("pid"));
}

#[test]
fn the_diffable_view_drops_a_volatile_value() {
    // Arrange
    let observation = Observation::object([
        ("command", Observation::text("nginx")),
        ("pid", Observation::integer(991).volatile()),
    ]);

    // Act
    let visible = observation
        .in_view(View::Diffable)
        .expect("a stable parent survives");

    // Assert
    let entries = entries_of(&visible);
    assert!(entries.contains_key("command"));
    assert!(
        !entries.contains_key("pid"),
        "a volatile value must not survive into the diffable view"
    );
}

#[test]
fn the_diffable_view_drops_a_whole_volatile_subtree() {
    // Arrange
    let observation = Observation::object([
        ("total_kb", Observation::integer(16_000_000)),
        (
            "counters",
            Observation::object([("free_kb", Observation::integer(812_344))]).volatile(),
        ),
    ]);

    // Act
    let visible = observation
        .in_view(View::Diffable)
        .expect("a stable parent survives");

    // Assert
    let entries = entries_of(&visible);
    assert!(entries.contains_key("total_kb"));
    assert!(
        !entries.contains_key("counters"),
        "marking a subtree volatile must drop everything under it"
    );
}

#[test]
fn the_diffable_view_drops_volatile_list_items() {
    // Arrange
    let observation = Observation::list([
        Observation::text("0.0.0.0:22"),
        Observation::text("127.0.0.1:41233").volatile(),
    ]);

    // Act
    let visible = observation
        .in_view(View::Diffable)
        .expect("a stable list survives");

    // Assert
    assert_eq!(items_of(&visible).len(), 1);
}

#[test]
fn the_diffable_view_drops_a_wholly_volatile_observation() {
    // Arrange
    let observation = Observation::text("42").volatile();

    // Act & Assert
    assert_eq!(observation.in_view(View::Diffable), None);
}
