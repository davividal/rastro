use rastro_fingerprint::observation::redaction::{REDACTION_PREFIX, redacted};
use rastro_fingerprint::{Observation, Presentation, Scalar, View, Xxh3Digest};

/// `sha256("password=hunter2")`, computed outside this workspace.
///
/// The point of an outside vector: it holds both stages at once. If the digest matched only
/// because both sides of the assertion called the same code, the test would pass with the
/// sha256 stage deleted, and the recipe is what a fingerprint archived before redaction
/// existed compares against.
const KNOWN_SHA256_HEX: &str = "42c842031e73fa74cb753b09101a15f4c0d7844be5f37bfa4af9b9ac435c6ca4";

fn text_of(observation: &Observation) -> String {
    match observation.content() {
        rastro_fingerprint::Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected a text observation, got {other:?}"),
    }
}

#[test]
fn a_redaction_is_xxh3_over_the_lowercase_sha256_hex() {
    // Arrange: the two stages, and the order between them, are the contract. PostgreSQL's
    // role digest was already this recipe with the server computing the sha256, so a
    // document taken before redaction existed still compares against one taken after. It
    // also pins text as the *untagged* domain, which is what stops a later tidy-up giving
    // it the type tag the other scalars carry.
    let secret = Scalar::Text("password=hunter2".to_owned());

    // Act
    let stand_in = redacted(&secret).expect("a text value has material to withhold");

    // Assert
    assert_eq!(
        stand_in,
        format!(
            "{REDACTION_PREFIX}{}",
            Xxh3Digest::of(KNOWN_SHA256_HEX.as_bytes()).as_str()
        ),
        "the xxh3 is taken over the hex characters, not the 32 raw bytes"
    );
}

#[test]
fn a_redaction_names_the_substitution_and_the_recipe() {
    // Arrange: a bare digest on a field whose name is not obviously a secret reads exactly
    // like a value, so the document says which it is.
    let secret = Scalar::Text("SCRAM-SHA-256$4096:salt$stored:server".to_owned());

    // Act
    let stand_in = redacted(&secret).expect("a text value has material to withhold");

    // Assert
    assert!(stand_in.starts_with("redacted:sha256+xxh3:"));
}

#[test]
fn a_scalar_that_changed_type_does_not_redact_to_the_same_stand_in() {
    // Arrange: without a domain tag these digest identically, and a value that changed type
    // would read as unchanged, which is the one thing the document exists to get right.
    let boolean = Scalar::Boolean(true);
    let text = Scalar::Text("true".to_owned());

    // Act
    let from_boolean = redacted(&boolean).expect("a boolean has material to withhold");
    let from_text = redacted(&text).expect("a text value has material to withhold");

    // Assert
    assert_ne!(from_boolean, from_text);
}

#[test]
fn an_integer_and_the_text_of_that_integer_redact_differently() {
    // Act
    let from_integer = redacted(&Scalar::Integer(1)).expect("an integer has material");
    let from_text = redacted(&Scalar::Text("1".to_owned())).expect("a text value has material");

    // Assert
    assert_ne!(from_integer, from_text);
}

#[test]
fn a_null_is_not_redacted_because_it_withholds_nothing() {
    // Act & Assert: digesting it would replace an honest absence with a stand-in for a
    // value that was never there.
    assert_eq!(redacted(&Scalar::Null), None);
}

#[test]
fn redaction_covers_everything_under_a_sensitive_subtree() {
    // Arrange: annotating a node covers everything under it, which is the rule volatility
    // already follows. A child of a withheld object is withheld whatever it says itself.
    let observation = Observation::object([(
        "connection",
        Observation::object([
            ("host", Observation::text("standby.internal")),
            ("password", Observation::text("hunter2")),
        ])
        .sensitive(),
    )]);

    // Act
    let visible = observation
        .in_view(Presentation::complete())
        .expect("a public parent survives");

    // Assert
    let connection = match visible.content() {
        rastro_fingerprint::Content::Object(entries) => entries["connection"].clone(),
        other => panic!("expected an object observation, got {other:?}"),
    };
    let entries = match connection.content() {
        rastro_fingerprint::Content::Object(entries) => entries.clone(),
        other => panic!("expected an object observation, got {other:?}"),
    };
    assert!(text_of(&entries["host"]).starts_with(REDACTION_PREFIX));
    assert!(text_of(&entries["password"]).starts_with(REDACTION_PREFIX));
}

#[test]
fn the_diffable_view_redacts_too_because_sensitivity_is_not_a_view() {
    // Arrange: a volatile value is dropped from one view and kept in the other; a sensitive
    // value is withheld from both, because the complete view is a fuller document and not a
    // way round an annotation.
    let observation = Observation::text("hunter2").sensitive();

    // Act
    let diffable = observation
        .in_view(View::Diffable)
        .expect("a stable value survives");
    let complete = observation
        .in_view(View::Complete)
        .expect("the complete view drops nothing");

    // Assert
    assert!(text_of(&diffable).starts_with(REDACTION_PREFIX));
    assert_eq!(text_of(&diffable), text_of(&complete));
}
