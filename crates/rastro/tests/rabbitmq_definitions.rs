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

/// A second export, from a node seeded with the entries the first one had none of: a shovel
/// and a federation upstream (each with a password in its URI), an operator policy, a policy
/// with a mixed-type definition, and a global parameter whose value is an Erlang proplist
/// carrying a float.
const RICH: &str = r#"{
  "bindings": [],
  "exchanges": [],
  "global_parameters": [
    {
      "name": "my-global",
      "value": [
        [
          "answer",
          42
        ],
        [
          "fraction",
          0.5
        ],
        [
          "on",
          true
        ]
      ]
    },
    {
      "name": "cluster_tags",
      "value": []
    }
  ],
  "parameters": [
    {
      "component": "operator_policy",
      "name": "capped",
      "value": {
        "apply-to": "queues",
        "definition": [
          [
            "max-length",
            5000
          ]
        ],
        "pattern": "^work",
        "priority": 0
      },
      "vhost": "/"
    },
    {
      "component": "shovel",
      "name": "my-shovel",
      "value": {
        "dest-protocol": "amqp091",
        "dest-queue": "out",
        "dest-uri": "amqp://localhost",
        "src-protocol": "amqp091",
        "src-queue": "in",
        "src-uri": "amqp://shovel-user:hunter2@upstream.example.com"
      },
      "vhost": "/"
    },
    {
      "component": "federation-upstream",
      "name": "my-upstream",
      "value": {
        "expires": 3600000,
        "uri": "amqp://fed-user:s3cret@peer.example.com"
      },
      "vhost": "/"
    }
  ],
  "permissions": [
    {
      "configure": ".*",
      "read": ".*",
      "user": "guest",
      "vhost": "/",
      "write": ".*"
    }
  ],
  "policies": [
    {
      "apply-to": "queues",
      "definition": {
        "dead-letter-exchange": "dlx",
        "max-length": 1000,
        "message-ttl": 60000,
        "queue-mode": "lazy"
      },
      "name": "mixed",
      "pattern": "^work",
      "priority": 2,
      "vhost": "/"
    }
  ],
  "queues": [],
  "rabbit_version": "4.0.5",
  "rabbitmq_version": "4.0.5",
  "topic_permissions": [],
  "users": [
    {
      "hashing_algorithm": "rabbit_password_hashing_sha256",
      "limits": {},
      "name": "guest",
      "password_hash": "fixtureAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
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
        "description": "Default virtual host",
        "tags": []
      },
      "name": "/"
    }
  ]
}"#;

/// The passwords inside that export's URIs. Fabricated, and the reason the withholding
/// assertions below are worth making.
const SHOVEL_PASSWORD: &str = "hunter2";
const UPSTREAM_PASSWORD: &str = "s3cret";

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

#[test]
fn parse_nests_a_permission_under_its_vhost_and_user() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let permissions = field(&Observation::from(&definitions), "permissions");

    // Assert: a permission is a fact about a pair, and neither half is unique on its own:
    // one user holds different permissions in each vhost, and one vhost grants different
    // permissions to each user. Nesting is what lets a reader open either question.
    assert_eq!(keys_of(&permissions), ["/", "spike"]);

    let spike = field(&field(&permissions, "spike"), "spikeuser");
    assert_eq!(text(&field(&spike, "configure")), "^spike.*");
    assert_eq!(text(&field(&spike, "write")), ".*");
    assert_eq!(text(&field(&spike, "read")), ".*");
}

#[test]
fn parse_nests_a_topic_permission_under_its_exchange_too() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let topics = field(&Observation::from(&definitions), "topic_permissions");

    // Assert: three levels, because a topic permission is per exchange as well: one user can
    // hold different routing-key patterns on `amq.topic` and on an exchange of their own.
    let spike = field(&field(&field(&topics, "spike"), "spikeuser"), "amq.topic");
    assert_eq!(text(&field(&spike, "write")), "^a");
    assert_eq!(text(&field(&spike, "read")), "^b");
}

#[test]
fn parse_keeps_a_permission_pattern_as_the_text_it_is() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let guest = field(
        &field(&field(&Observation::from(&definitions), "permissions"), "/"),
        "guest",
    );

    // Assert: `.*` is a regular expression the broker compiles, and rastro neither compiles
    // nor normalises it. What changed is whether the text changed, which is the question a
    // fingerprint answers, and a normalised pattern would hide an edit that meant the same
    // thing while still being a change somebody made.
    assert_eq!(text(&field(&guest, "configure")), ".*");
}

#[test]
fn parse_keys_a_policy_by_vhost_and_name() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(RICH).expect("well formed");
    let policies = field(&Observation::from(&definitions), "policies");

    // Assert
    let mixed = field(&field(&policies, "/"), "mixed");
    assert_eq!(text(&field(&mixed, "pattern")), "^work");
    assert_eq!(text(&field(&mixed, "apply_to")), "queues");
    assert_eq!(support::observation::integer(&field(&mixed, "priority")), 2);
}

#[test]
fn parse_keeps_a_policy_definitions_own_types() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(RICH).expect("well formed");
    let definition = field(
        &field(&field(&Observation::from(&definitions), "policies"), "/"),
        "mixed",
    );
    let body = field(&definition, "definition");

    // Assert: a number stays a number and text stays text, because a value that changed type
    // would otherwise read as unchanged.
    assert_eq!(
        support::observation::integer(&field(&body, "max-length")),
        1000
    );
    assert_eq!(text(&field(&body, "queue-mode")), "lazy");
}

#[test]
fn parse_withholds_every_runtime_parameter_value() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(RICH).expect("well formed");
    let parameters = field(&Observation::from(&definitions), "parameters");

    // Assert: a shovel and a federation upstream keep their peer's password inside a URI, so
    // every value is withheld and none is judged by the name of its component. The same rule
    // the container facet applies to an environment variable, for the same reason: a plugin
    // can define any component, and a future one can put a credential anywhere in it.
    for (component, name) in [
        ("shovel", "my-shovel"),
        ("federation-upstream", "my-upstream"),
        ("operator_policy", "capped"),
    ] {
        let value = field(
            &field(&field(&field(&parameters, "/"), component), name),
            "value",
        );

        assert_eq!(
            value.sensitivity(),
            Sensitivity::Sensitive,
            "{component}/{name} must be withheld"
        );
    }
}

#[test]
fn parse_reports_an_operator_policy_as_the_parameter_it_is_exported_as() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(RICH).expect("well formed");
    let parameters = field(&Observation::from(&definitions), "parameters");

    // Assert: measured rather than assumed. There is no `operator_policies` key in the
    // export at all: an operator policy arrives as a runtime parameter whose component is
    // `operator_policy`, so reporting it anywhere else would invent a shape RabbitMQ does
    // not have.
    assert!(
        keys_of(&field(&field(&parameters, "/"), "operator_policy")).contains(&"capped".to_owned())
    );
}

#[test]
fn parse_withholds_a_global_parameter_value_too() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(RICH).expect("well formed");
    let globals = field(&Observation::from(&definitions), "global_parameters");

    // Assert: one of these carries `[["answer",42],["fraction",0.5],["on",true]]`, an Erlang
    // proplist with a float in it. Withholding the value as its own text spelling settles
    // both problems at once: no shape to interpret, and no float in the document.
    assert_eq!(
        field(&field(&globals, "my-global"), "value").sensitivity(),
        Sensitivity::Sensitive
    );
    assert!(keys_of(&globals).contains(&"cluster_tags".to_owned()));
}

#[test]
fn the_rich_fixture_holds_real_passwords_so_the_withholding_is_worth_asserting() {
    // Act & Assert: the same guard the verifier gets. If the fixture ever loses its
    // credentials, every assertion above would pass while proving nothing.
    assert!(RICH.contains(SHOVEL_PASSWORD));
    assert!(RICH.contains(UPSTREAM_PASSWORD));
}

#[test]
fn parse_keys_an_exchange_by_vhost_and_name() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let exchanges = field(&Observation::from(&definitions), "exchanges");

    // Assert
    let direct = field(&field(&exchanges, "spike"), "spike.direct");
    assert_eq!(text(&field(&direct, "exchange_type")), "direct");
    assert!(support::observation::boolean(&field(&direct, "durable")));
    assert!(!support::observation::boolean(&field(
        &direct,
        "auto_delete"
    )));
}

#[test]
fn parse_records_the_type_a_queue_ended_up_with_beside_the_argument_that_asked() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let work = field(
        &field(&field(&Observation::from(&definitions), "queues"), "spike"),
        "work",
    );

    // Assert: the export carries both, and they are different facts. `x-queue-type` is what
    // the declaration asked for; `type` is what the broker made, which a vhost's default or a
    // policy can decide instead.
    assert_eq!(text(&field(&work, "queue_type")), "quorum");
    assert_eq!(
        text(&field(&field(&work, "arguments"), "x-queue-type")),
        "quorum"
    );
    assert!(support::observation::boolean(&field(&work, "durable")));
}

#[test]
fn parse_lists_the_bindings_of_a_vhost_rather_than_keying_them() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let bindings = items_of(&field(
        &field(&Observation::from(&definitions), "bindings"),
        "spike",
    ));

    // Assert: a binding has no unique name. One source and one destination can be bound
    // several times over with different routing keys, and a headers exchange can carry two
    // bindings with the same routing key and different arguments, so there is no key that
    // would not collide. A list with a defined order is the honest shape.
    assert_eq!(bindings.len(), 1);
    assert_eq!(text(&field(&bindings[0], "source")), "spike.direct");
    assert_eq!(text(&field(&bindings[0], "destination")), "work");
    assert_eq!(text(&field(&bindings[0], "destination_type")), "queue");
    assert_eq!(text(&field(&bindings[0], "routing_key")), "k");
}

#[test]
fn parse_reports_a_vhost_with_no_topology_as_having_none() {
    // Act
    let definitions = RabbitmqctlDefinitions::parse(MEASURED).expect("well formed");
    let rendered = Observation::from(&definitions);

    // Assert: the default vhost held nothing durable on the measured box, and the export says
    // so by not mentioning it. An absent key is the reading, not a gap: `/` is in `vhosts`
    // where a reader can see it exists.
    assert!(!keys_of(&field(&rendered, "queues")).contains(&"/".to_owned()));
    assert!(keys_of(&field(&rendered, "vhosts")).contains(&"/".to_owned()));
}

#[test]
fn parse_drops_an_entry_the_export_could_not_name() {
    // Arrange: every shape whose key is missing. Only a RabbitMQ that changed the document
    // produces these, and one unnameable entry is not worth every other entry in the export.
    let nameless = r#"{"rabbitmq_version":"4.0.5",
      "vhosts":[{"default_queue_type":"classic"}],
      "users":[{"tags":["administrator"]}],
      "permissions":[{"vhost":"/","configure":".*"}],
      "topic_permissions":[{"vhost":"/","user":"guest"}],
      "policies":[{"vhost":"/","pattern":"^x"}],
      "parameters":[{"vhost":"/","name":"nameless"}],
      "global_parameters":[{"value":{}}],
      "exchanges":[{"vhost":"/","type":"direct"}],
      "queues":[{"vhost":"/","type":"quorum"}],
      "bindings":[{"source":"x","destination":"y"}]}"#;

    // Act
    let definitions = RabbitmqctlDefinitions::parse(nameless).expect("a readable document");

    // Assert: read, and empty, rather than failed.
    assert!(definitions.vhosts.is_empty());
    assert!(definitions.users.is_empty());
    assert!(definitions.permissions.is_empty());
    assert!(definitions.topic_permissions.is_empty());
    assert!(definitions.policies.is_empty());
    assert!(definitions.parameters.is_empty());
    assert!(definitions.global_parameters.is_empty());
    assert!(definitions.exchanges.is_empty());
    assert!(definitions.queues.is_empty());
    assert!(definitions.bindings.is_empty());
}

#[test]
fn parse_reads_past_a_preamble_here_too() {
    // Arrange: the same runtime report that precedes a status document can precede this one,
    // and both reads go through the same tool.
    let noisy = format!(
        "=ERROR REPORT==== 23-Sep-2026::14:03:05.184639 ===\nfile:path_eval([]): permission denied\n\n{MEASURED}"
    );

    // Act & Assert
    assert!(RabbitmqctlDefinitions::parse(&noisy).is_ok());
}

#[test]
fn parse_reads_an_empty_string_where_a_collection_belongs_as_no_entries() {
    // Arrange: a node in the fleet exported `""` for a collection with nothing in it, and it
    // failed the whole facet with "expected a sequence at line 1 column 4889" — an offset into
    // a document nobody keeps, naming neither the field nor the node. Every collection carries
    // the shape at once here, because the reader must not care which of them arrives that way.
    let empty_strings = r#"{"rabbitmq_version":"3.10.8",
      "vhosts":"","users":"","permissions":"","topic_permissions":"","policies":"",
      "parameters":"","global_parameters":"","exchanges":"","queues":"","bindings":""}"#;

    // Act
    let definitions = RabbitmqctlDefinitions::parse(empty_strings).expect("a readable document");

    // Assert: read, and empty, rather than failed.
    assert!(definitions.vhosts.is_empty());
    assert!(definitions.users.is_empty());
    assert!(definitions.permissions.is_empty());
    assert!(definitions.topic_permissions.is_empty());
    assert!(definitions.policies.is_empty());
    assert!(definitions.parameters.is_empty());
    assert!(definitions.global_parameters.is_empty());
    assert!(definitions.exchanges.is_empty());
    assert!(definitions.queues.is_empty());
    assert!(definitions.bindings.is_empty());
}

#[test]
fn parse_refuses_text_where_a_collection_belongs() {
    // Arrange: the empty string is read as "nothing here". Text that says something else is a
    // document rastro does not understand, and a guess at it would be worse than a failure.
    let text = r#"{"rabbitmq_version":"3.10.8","users":"guest,spikeuser"}"#;

    // Act & Assert
    assert!(RabbitmqctlDefinitions::parse(text).is_err());
}

#[test]
fn parse_reads_tags_exported_as_one_comma_separated_string() {
    // Arrange: a user's tags are a JSON list from 3.9 onward and one comma-separated string
    // before it, and a node upgraded across that line can still carry the older spelling. A
    // user with no tags exports it as the empty string.
    let older = r#"{"rabbitmq_version":"3.10.8","users":[
      {"name":"guest","tags":"administrator,management",
       "hashing_algorithm":"rabbit_password_hashing_sha256",
       "password_hash":"guestAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"},
      {"name":"plain","tags":"",
       "hashing_algorithm":"rabbit_password_hashing_sha256",
       "password_hash":"plainAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]}"#;

    // Act
    let definitions = RabbitmqctlDefinitions::parse(older).expect("a readable document");

    // Assert
    assert_eq!(
        definitions.users["guest"].tags,
        ["administrator".to_owned(), "management".to_owned()]
    );
    assert!(definitions.users["plain"].tags.is_empty());
}

#[test]
fn parse_names_the_field_a_document_failed_at() {
    // Arrange: a shape this reader does not anticipate, which is the case that matters —
    // anticipated ones are read. A byte offset into a document of several megabytes that
    // nothing keeps a copy of tells an operator nothing they can act on; the field does.
    let unreadable = r#"{"rabbitmq_version":"3.10.8","queues":[
      {"name":"work","vhost":"/","arguments":[]}]}"#;

    // Act
    let failure =
        RabbitmqctlDefinitions::parse(unreadable).expect_err("a list is not a map of arguments");

    // Assert
    let message = failure.to_string();
    assert!(
        message.contains("queues[0].arguments: invalid type: sequence, expected a map"),
        "{message}"
    );
}
