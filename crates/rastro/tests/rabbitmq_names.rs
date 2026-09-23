//! The facet's leaf values: what they refuse, and what they say about themselves.

use rastro::collectors::rabbitmq::{BrokerEvidence, NodeName, PasswordHashing};

#[test]
fn a_node_name_is_whatever_the_node_runs_under() {
    // Act
    let short = NodeName::parse("rabbit@box").expect("a name");
    let long = NodeName::parse("rabbit@broker.example.test").expect("a name");

    // Assert: read, not composed. A node under long names calls itself the second of these,
    // and an earlier version that built `local@hostname` would have said `rabbit@broker`.
    assert_eq!(short.as_str(), "rabbit@box");
    assert_eq!(long.as_str(), "rabbit@broker.example.test");
}

#[test]
fn a_node_name_refuses_what_no_node_answers_to() {
    // Act & Assert: both halves, and exactly one separator.
    assert!(NodeName::parse("rabbit").is_err());
    assert!(NodeName::parse("@box").is_err());
    assert!(NodeName::parse("rabbit@").is_err());
    assert!(NodeName::parse("rabbit@box@extra").is_err());
}

#[test]
fn node_names_order_by_the_name_they_are() {
    // Arrange
    let mut names = [
        NodeName::parse("rabbit@box").expect("legal"),
        NodeName::parse("aardvark@box").expect("legal"),
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
