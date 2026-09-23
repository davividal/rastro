//! The facet's leaf values: what they refuse, and what they say about themselves.

use rastro::collectors::rabbitmq::{BrokerEvidence, NodeName, PasswordHashing};

#[test]
fn a_node_name_is_the_local_part_joined_to_the_host() {
    // Act
    let name = NodeName::new("rabbit", "box").expect("both halves are there");

    // Assert: what `rabbitmqctl -n` takes, and the facet's key.
    assert_eq!(name.as_str(), "rabbit@box");
}

#[test]
fn a_node_name_refuses_a_half_that_already_carries_the_separator() {
    // Act & Assert: `a@b@c` cannot be split back into the halves that made it, and the
    // halves are what a CLI tool is addressed with.
    assert!(NodeName::new("rabbit@already", "box").is_err());
    assert!(NodeName::new("rabbit", "box@already").is_err());
}

#[test]
fn a_node_name_refuses_an_empty_half() {
    // Act & Assert: `@box` and `rabbit@` address nothing.
    assert!(NodeName::new("", "box").is_err());
    assert!(NodeName::new("rabbit", "").is_err());
}

#[test]
fn node_names_order_by_the_key_they_render_as() {
    // Arrange
    let mut names = [
        NodeName::new("rabbit", "box").expect("legal"),
        NodeName::new("aardvark", "box").expect("legal"),
    ];

    // Act
    names.sort();

    // Assert: the same order the document's own keys are in, so the two cannot disagree.
    assert_eq!(names[0].as_str(), "aardvark@box");
}

#[test]
fn a_hashing_scheme_is_named_by_what_rastro_calls_it() {
    // Act & Assert: the Erlang module prefix is dropped, because a reader of the document is
    // not reading Erlang.
    assert_eq!(
        PasswordHashing::parse("rabbit_password_hashing_sha256").as_str(),
        "sha256"
    );
    assert_eq!(
        PasswordHashing::parse("rabbit_password_hashing_sha512").as_str(),
        "sha512"
    );
    assert_eq!(
        PasswordHashing::parse("rabbit_password_hashing_md5").as_str(),
        "md5"
    );
}

#[test]
fn a_scheme_nobody_has_read_keeps_its_own_spelling() {
    // Act
    let unread = PasswordHashing::parse("rabbit_password_hashing_argon2");

    // Assert: rastro has no name for something it has not read, and shortening it would
    // invent one.
    assert_eq!(unread.as_str(), "rabbit_password_hashing_argon2");
}

#[test]
fn only_the_two_schemes_somebody_checked_carry_a_verifier() {
    // Act & Assert: fail closed. Every scheme RabbitMQ ships salts the password with 32
    // bits, so what separates these is the cost of testing one candidate against the
    // stand-in, which under md5 is seconds of ordinary GPU time.
    assert!(PasswordHashing::parse("rabbit_password_hashing_sha256").carries_verifier());
    assert!(PasswordHashing::parse("rabbit_password_hashing_sha512").carries_verifier());
    assert!(!PasswordHashing::parse("rabbit_password_hashing_md5").carries_verifier());
    assert!(!PasswordHashing::parse("rabbit_password_hashing_argon2").carries_verifier());
}

#[test]
fn evidence_answers_the_tri_state_the_document_renders() {
    // Act & Assert: the two `None` cases are the reason this type exists. A boolean reported
    // a live broker as not a broker when rastro could not read its descriptors.
    assert_eq!(BrokerEvidence::RabbitmqProcess.runs_rabbitmq(), Some(true));
    assert_eq!(
        BrokerEvidence::OtherApplication.runs_rabbitmq(),
        Some(false)
    );
    assert_eq!(BrokerEvidence::NotOffered.runs_rabbitmq(), Some(false));
    assert_eq!(BrokerEvidence::HolderUnreadable.runs_rabbitmq(), None);
    assert_eq!(BrokerEvidence::TablesUnreadable.runs_rabbitmq(), None);
}

#[test]
fn only_a_confirmed_broker_may_be_addressed() {
    // Act & Assert: addressing another application's node makes it log an authentication
    // failure, and addressing one rastro knows nothing about is the same gamble with less
    // information.
    assert!(BrokerEvidence::RabbitmqProcess.may_be_addressed());

    for evidence in [
        BrokerEvidence::OtherApplication,
        BrokerEvidence::NotOffered,
        BrokerEvidence::HolderUnreadable,
        BrokerEvidence::TablesUnreadable,
    ] {
        assert!(
            !evidence.may_be_addressed(),
            "{evidence:?} must not be addressed"
        );
    }
}

#[test]
fn every_evidence_says_something_different_in_the_document() {
    // Arrange
    let spellings: Vec<&str> = [
        BrokerEvidence::RabbitmqProcess,
        BrokerEvidence::OtherApplication,
        BrokerEvidence::NotOffered,
        BrokerEvidence::HolderUnreadable,
        BrokerEvidence::TablesUnreadable,
    ]
    .iter()
    .map(BrokerEvidence::as_str)
    .collect();

    // Act
    let mut unique = spellings.clone();
    unique.sort_unstable();
    unique.dedup();

    // Assert: the evidence is what a reader acts on, so two host states that differ must not
    // read the same.
    assert_eq!(unique.len(), spellings.len());
}
