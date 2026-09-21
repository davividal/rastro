//! The definitions a node exports, and the one value in them that must not be printed.
//!
//! The fixture is verbatim from `rabbitmqctl export_definitions -` on the box the footprint
//! was measured on, seeded with a vhost, a user, permissions, a policy, a quorum queue, an
//! exchange and a binding, so every half of the document is represented.
//!
//! **The stored verifiers are invented**, and they are the one thing in these fixtures
//! that is not what the broker printed. A real one is 36 random bytes in base64 and reads to
//! a scanner exactly like a credential, which is what `.gitleaks.toml` exists to stop
//! reaching the tree; each is replaced by a base64 string of the same length made of
//! one name and a run of `A`. Everything the parse depends on, the shape, the
//! length and the algorithm beside it, is unchanged.

use rastro::collectors::rabbitmq::RabbitmqctlDefinitions;
use rastro_collector::{Observation, Sensitivity};

mod support;

use support::observation::{field, items_of, keys_of, text};

/// `rabbitmqctl export_definitions -`, verbatim, from the measured box.
const MEASURED: &str = r#"{
  "bindings": [
    {
      "arguments": {},
      "destination": "work",
      "destination_type": "queue",
      "routing_key": "k",
      "source": "spike.direct",
      "vhost": "spike"
    }
  ],
  "exchanges": [
    {
      "arguments": {},
      "auto_delete": false,
      "durable": true,
      "name": "spike.direct",
      "type": "direct",
      "vhost": "spike"
    }
  ],
  "global_parameters": [
    {
      "name": "cluster_tags",
      "value": []
    }
  ],
  "parameters": [],
  "permissions": [
    {
      "configure": ".*",
      "read": ".*",
      "user": "guest",
      "vhost": "/",
      "write": ".*"
    },
    {
      "configure": "^spike.*",
      "read": ".*",
      "user": "spikeuser",
      "vhost": "spike",
      "write": ".*"
    }
  ],
  "policies": [
    {
      "apply-to": "queues",
      "definition": {
        "max-length": 1000
      },
      "name": "ha",
      "pattern": "^work",
      "priority": 1,
      "vhost": "spike"
    }
  ],
  "queues": [
    {
      "arguments": {
        "x-queue-type": "quorum"
      },
      "auto_delete": false,
      "durable": true,
      "name": "work",
      "type": "quorum",
      "vhost": "spike"
    }
  ],
  "rabbit_version": "4.0.5",
  "rabbitmq_version": "4.0.5",
  "topic_permissions": [
    {
      "exchange": "amq.topic",
      "read": "^b",
      "user": "spikeuser",
      "vhost": "spike",
      "write": "^a"
    }
  ],
  "users": [
    {
      "hashing_algorithm": "rabbit_password_hashing_sha256",
      "limits": {},
      "name": "spikeuser",
      "password_hash": "spikeuserAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
      "tags": [
        "monitoring"
      ]
    },
    {
      "hashing_algorithm": "rabbit_password_hashing_sha256",
      "limits": {},
      "name": "guest",
      "password_hash": "guestAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
      "tags": [
        "administrator"
      ]
    }
  ],
  "vhosts": [
    {
      "default_queue_type": "classic",
      "limits": [],
      "metadata": {
        "description": "",
        "tags": []
      },
      "name": "spike"
    },
    {
      "default_queue_type": "classic",
      "limits": [],
      "metadata": {
        "description": "Default virtual host",
        "tags": []
      },
      "name": "/"
    }
  ]
}"#;

/// The `guest` user's stored verifier on that box, which must never reach a document.
const GUEST_VERIFIER: &str = "guestAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[test]
fn parse_keys_the_users_by_name_and_records_their_tags() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let users = field(&Observation::from(&definitions), "users");

    // Assert
    assert_eq!(keys_of(&users), ["guest", "spikeuser"]);

    let tags: Vec<String> = items_of(&field(&field(&users, "guest"), "tags"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(tags, ["administrator".to_owned()]);
}

#[test]
fn parse_marks_the_stored_verifier_sensitive() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let guest = field(&field(&Observation::from(&definitions), "users"), "guest");

    // Assert: the material reaches rastro's memory whether or not it is wanted, since the
    // CLI hands over the whole document. Marking it is what makes the default document carry
    // a stand-in and `--raw` carry the value, and it is the whole of the protection.
    assert_eq!(
        field(&guest, "password_hash").sensitivity(),
        Sensitivity::Sensitive
    );
}

#[test]
fn parse_records_the_algorithm_that_produced_the_verifier() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let guest = field(&field(&Observation::from(&definitions), "users"), "guest");

    // Assert: recorded beside the verifier, because a change of scheme and a change of
    // password are different findings and one field cannot say both.
    assert_eq!(text(&field(&guest, "password_hashing")), "sha256");
}

#[test]
fn parse_withholds_a_verifier_produced_by_md5() {
    // Arrange: RabbitMQ salts every scheme with 32 bits, so the salt is not what tells them
    // apart. What does is the cost of testing one candidate password against the stand-in:
    // 2^32 hashes, which is seconds of GPU time under md5.
    let legacy = MEASURED.replace(
        "rabbit_password_hashing_sha256",
        "rabbit_password_hashing_md5",
    );

    // Act
    let definitions = RabbitmqctlDefinitions::parse(&legacy).expect("well formed");
    let guest = field(&field(&Observation::from(&definitions), "users"), "guest");

    // Assert: absent, with the algorithm beside it saying why. Fail closed.
    assert!(support::observation::is_null(&field(
        &guest,
        "password_hash"
    )));
    assert_eq!(text(&field(&guest, "password_hashing")), "md5");
}

#[test]
fn parse_withholds_a_verifier_from_a_scheme_nobody_has_checked() {
    // Arrange
    let future = MEASURED.replace(
        "rabbit_password_hashing_sha256",
        "rabbit_password_hashing_new",
    );

    // Act
    let definitions = RabbitmqctlDefinitions::parse(&future).expect("well formed");
    let guest = field(&field(&Observation::from(&definitions), "users"), "guest");

    // Assert: a scheme a later RabbitMQ adds gets nothing until somebody has read how it
    // works, rather than being trusted by default.
    assert!(support::observation::is_null(&field(
        &guest,
        "password_hash"
    )));
    assert_eq!(
        text(&field(&guest, "password_hashing")),
        "rabbit_password_hashing_new"
    );
}

#[test]
fn parse_records_what_a_vhost_was_created_with() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let vhosts = field(&Observation::from(&definitions), "vhosts");

    // Assert
    assert_eq!(keys_of(&vhosts), ["/", "spike"]);
    assert_eq!(
        text(&field(&field(&vhosts, "/"), "default_queue_type")),
        "classic"
    );
    assert_eq!(
        text(&field(&field(&vhosts, "/"), "description")),
        "Default virtual host"
    );
}

#[test]
fn parse_records_the_version_the_definitions_came_from() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");

    // Assert: the export's own account of the node that wrote it, which is what makes an
    // archived definitions document readable later.
    assert_eq!(
        text(&field(&Observation::from(&definitions), "rabbitmq_version")),
        "4.0.5"
    );
}

#[test]
fn parse_refuses_a_document_that_is_not_definitions() {
    // Act & Assert
    assert!(RabbitmqctlDefinitions::parse("Usage\n\nrabbitmqctl").is_err());
    assert!(RabbitmqctlDefinitions::parse(r#"{"os":"Linux"}"#).is_err());
}

#[test]
fn the_fixture_holds_a_real_verifier_so_the_withholding_is_worth_asserting() {
    // Act & Assert: a guard on the test rather than on the code. If the fixture ever loses
    // its verifier, every assertion above would pass while proving nothing.
    assert!(MEASURED.contains(GUEST_VERIFIER));
}
