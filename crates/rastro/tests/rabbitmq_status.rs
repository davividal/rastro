//! The node's own account of itself, parsed from a status somebody captured.
//!
//! The fixture is verbatim from `rabbitmqctl status --formatter json` on the box the
//! footprint was measured on: RabbitMQ 4.0.5 on Debian 13, Erlang 27. Kept whole, volatile
//! halves included, because half of what this parse has to get right is what it leaves out.

use rastro::collectors::rabbitmq::{RabbitmqctlStatus, document_in};
use rastro_collector::{Observation, Volatility};

mod support;

use support::observation::{boolean, field, integer, is_null, items_of, keys_of, text};

/// `rabbitmqctl status --formatter json`, verbatim, from the measured box.
const MEASURED: &str = r#"{
  "memory": {
    "atom": 991489,
    "binary": 25744,
    "code": 17047704,
    "total": {
      "erlang": 55301842,
      "rss": 106852352,
      "allocated": 62435328
    },
    "strategy": "rss",
    "plugins": 13200,
    "mnesia": 83976,
    "connection_readers": 0,
    "connection_writers": 0,
    "connection_channels": 0,
    "connection_other": 0,
    "queue_procs": 0,
    "quorum_queue_procs": 48916,
    "quorum_queue_dlx_procs": 2752,
    "stream_queue_procs": 1304,
    "stream_queue_replica_reader_procs": 1304,
    "stream_queue_coordinator_procs": 0,
    "metadata_store": 42668,
    "other_proc": 14883320,
    "metrics": 1113288,
    "mgmt_db": 0,
    "quorum_ets": 37480,
    "metadata_store_ets": 7944,
    "other_ets": 1648528,
    "msg_index": 417888,
    "other_system": 18934337,
    "allocated_unused": 7133486,
    "reserved_unallocated": 44417024
  },
  "pid": 966,
  "processes": {
    "used": 322,
    "limit": 1048576
  },
  "run_queue": 1,
  "os": "Linux",
  "net_ticktime": 60,
  "alarms": [],
  "tags": [],
  "listeners": [
    {
      "node": "rabbit@270fcab787e4",
      "port": 25672,
      "protocol": "clustering",
      "interface": "[::]",
      "purpose": "inter-node and CLI tool communication"
    },
    {
      "node": "rabbit@270fcab787e4",
      "port": 5672,
      "protocol": "amqp",
      "interface": "[::]",
      "purpose": "AMQP 0-9-1 and AMQP 1.0"
    }
  ],
  "product_name": "",
  "product_version": "",
  "erlang_version": "Erlang/OTP 27 [erts-15.2.7] [source] [64-bit] [smp:7:7] [ds:7:7:10] [async-threads:1] [jit]",
  "rabbitmq_version": "4.0.5",
  "uptime": 14,
  "is_under_maintenance": false,
  "crypto_lib_version": "OpenSSL 3.5.7 9 Jun 2026",
  "enabled_plugin_file": "/etc/rabbitmq/enabled_plugins",
  "active_plugins": [],
  "data_directory": "/var/lib/rabbitmq/mnesia/rabbit@270fcab787e4",
  "raft_data_directory": "/var/lib/rabbitmq/mnesia/rabbit@270fcab787e4/quorum/rabbit@270fcab787e4",
  "config_files": [],
  "log_files": [
    "/var/log/rabbitmq/rabbit@270fcab787e4.log",
    "<stdout>"
  ],
  "vm_memory_calculation_strategy": "rss",
  "vm_memory_high_watermark_setting": {
    "relative": 0.6
  },
  "vm_memory_high_watermark_limit": 4623935078,
  "file_descriptors": {
    "total_used": 0,
    "total_limit": 927,
    "sockets_limit": 0,
    "sockets_used": 0
  },
  "disk_free_limit": 50000000,
  "disk_free": 51965030400,
  "totals": {
    "virtual_host_count": 2,
    "connection_count": 0,
    "queue_count": 1
  }
}"#;

#[test]
fn parse_reads_what_the_node_says_it_is() {
    // Act
    let status = RabbitmqctlStatus::parse(MEASURED).expect("well formed");
    let rendered = Observation::from(&status);

    // Assert
    assert_eq!(text(&field(&rendered, "rabbitmq_version")), "4.0.5");
    assert!(text(&field(&rendered, "erlang_version")).starts_with("Erlang/OTP 27"));
    assert_eq!(text(&field(&rendered, "operating_system")), "Linux");
    assert_eq!(
        text(&field(&rendered, "data_directory")),
        "/var/lib/rabbitmq/mnesia/rabbit@270fcab787e4"
    );
}

#[test]
fn parse_takes_the_node_name_from_a_listener() {
    // Act
    let status = RabbitmqctlStatus::parse(MEASURED).expect("well formed");

    // Assert: the status document carries no node name of its own, and every listener
    // carries the node it belongs to. This is the node's *own* account of its name, beside
    // the one rastro composed from the register and the hostname, so the two can disagree.
    assert_eq!(status.reported_name.as_deref(), Some("rabbit@270fcab787e4"));
}

#[test]
fn parse_keeps_the_volatile_half_out_of_the_document() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(MEASURED).expect("well formed"));

    // Assert: memory, uptime, the pid, the run queue, the descriptor count, free disk and
    // the connection totals all move between two runs of an unchanged box. They are not
    // annotated volatile, they are not read: a fingerprint is not a monitoring tool, and a
    // default view carrying a memory reading teaches the operator that the tool is noisy.
    for absent in [
        "memory",
        "uptime",
        "pid",
        "run_queue",
        "processes",
        "file_descriptors",
        "disk_free",
        "totals",
    ] {
        assert!(
            !keys_of(&rendered).contains(&absent.to_owned()),
            "{absent:?} moves on its own and must not be in the facet"
        );
    }
}

#[test]
fn parse_records_the_files_the_node_is_actually_using() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(MEASURED).expect("well formed"));

    // Assert: the measured box had no rabbitmq.conf at all, which is the ordinary state of a
    // stock Debian install and the reason the node's own account is the only account there
    // is. An empty list says rastro asked and the node named none.
    assert!(items_of(&field(&rendered, "configuration_files")).is_empty());

    // `<stdout>` is in the log list beside a real path, which is why these are destinations
    // rather than files.
    let destinations: Vec<String> = items_of(&field(&rendered, "log_destinations"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(
        destinations,
        [
            "/var/log/rabbitmq/rabbit@270fcab787e4.log".to_owned(),
            "<stdout>".to_owned()
        ]
    );

    assert_eq!(
        text(&field(&rendered, "enabled_plugins_file")),
        "/etc/rabbitmq/enabled_plugins"
    );
}

#[test]
fn parse_records_every_listener_the_node_bound() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(MEASURED).expect("well formed"));

    // Assert: what the node says it is listening on, which is a different fact from what the
    // `sockets` facet observed bound, and kept apart so the two can disagree.
    let listeners = items_of(&field(&rendered, "listeners"));
    assert_eq!(listeners.len(), 2);

    let clustering = &listeners[0];
    assert_eq!(text(&field(clustering, "protocol")), "clustering");
    assert_eq!(integer(&field(clustering, "port")), 25672);
    assert_eq!(text(&field(clustering, "interface")), "[::]");
}

#[test]
fn parse_reports_an_empty_product_name_as_absent() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(MEASURED).expect("well formed"));

    // Assert: the open-source build prints `""` for both product fields, and an empty string
    // asserts something the node did not say. Null is the honest rendering.
    assert!(is_null(&field(&rendered, "product_name")));
    assert!(is_null(&field(&rendered, "product_version")));
}

#[test]
fn parse_records_the_resolved_watermark_and_not_the_setting() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(MEASURED).expect("well formed"));

    // Assert: `vm_memory_high_watermark_setting` is `{"relative": 0.6}` and the format admits
    // no floating point, so the resolved byte limit is what reaches the document. It is state
    // either way: it moves when the setting moves, or when the box gains memory.
    assert_eq!(
        integer(&field(&rendered, "memory_high_watermark_limit")),
        4623935078
    );
    assert_eq!(integer(&field(&rendered, "disk_free_limit")), 50000000);
    assert_eq!(integer(&field(&rendered, "net_tick_seconds")), 60);
    assert!(!boolean(&field(&rendered, "under_maintenance")));
}

#[test]
fn parse_refuses_a_document_that_is_not_json() {
    // Act & Assert: a CLI tool that printed a usage dump or an error page is a failed read,
    // never an empty status.
    assert!(RabbitmqctlStatus::parse("Usage\n\nrabbitmqctl [--node <node>]").is_err());
}

#[test]
fn parse_refuses_a_status_that_names_no_rabbitmq_version() {
    // Act & Assert: the safety check, not a formality. The register lists every Erlang node
    // on the box, so a status without a RabbitMQ version is evidence the answer came from
    // something that is not a broker, and recording it as one would put another
    // application's state in this facet.
    assert!(
        RabbitmqctlStatus::parse(r#"{"os":"Linux","erlang_version":"Erlang/OTP 27"}"#).is_err()
    );
}

/// `rabbitmqctl status --formatter json` from RabbitMQ **3.12.1** on Ubuntu 24.04, verbatim.
/// Older than the box the rest of these fixtures came from, and different in two ways that
/// matter: it carries `release_series_support_status`, which 4.0 does not, and it has no
/// `tags` key at all.
const OLDER: &str = r#"{
  "active_plugins": [],
  "alarms": [],
  "config_files": [],
  "crypto_lib_version": "OpenSSL 3.0.13 30 Jan 2024",
  "data_directory": "/var/lib/rabbitmq/mnesia/rabbit@c75013d4b192",
  "disk_free": 51814764544,
  "disk_free_limit": 50000000,
  "enabled_plugin_file": "/etc/rabbitmq/enabled_plugins",
  "erlang_version": "Erlang/OTP 25 [erts-13.2.2.5] [source] [64-bit] [smp:7:7] [ds:7:7:10] [async-threads:1] [jit]",
  "file_descriptors": {
    "sockets_limit": 943629,
    "sockets_used": 0,
    "total_limit": 1048479,
    "total_used": 2
  },
  "is_under_maintenance": false,
  "listeners": [
    {
      "interface": "[::]",
      "node": "rabbit@c75013d4b192",
      "port": 25672,
      "protocol": "clustering",
      "purpose": "inter-node and CLI tool communication"
    },
    {
      "interface": "[::]",
      "node": "rabbit@c75013d4b192",
      "port": 5672,
      "protocol": "amqp",
      "purpose": "AMQP 0-9-1 and AMQP 1.0"
    }
  ],
  "log_files": [
    "/var/log/rabbitmq/rabbit@c75013d4b192.log",
    "<stdout>"
  ],
  "memory": {
    "allocated_unused": 0,
    "atom": 1376577,
    "binary": 160464,
    "code": 28577582,
    "connection_channels": 0,
    "connection_other": 0,
    "connection_readers": 0,
    "connection_writers": 0,
    "metrics": 966944,
    "mgmt_db": 0,
    "mnesia": 73264,
    "msg_index": 209936,
    "other_ets": 2207288,
    "other_proc": 18095008,
    "other_system": 16231209,
    "plugins": 34040,
    "queue_procs": 0,
    "queue_slave_procs": 0,
    "quorum_ets": 25480,
    "quorum_queue_dlx_procs": 2760,
    "quorum_queue_procs": 2760,
    "reserved_unallocated": 85037056,
    "strategy": "rss",
    "stream_queue_coordinator_procs": 0,
    "stream_queue_procs": 1312,
    "stream_queue_replica_reader_procs": 1312,
    "total": {
      "allocated": 52629504,
      "erlang": 67965936,
      "rss": 137666560
    }
  },
  "net_ticktime": 60,
  "os": "Linux",
  "pid": 1085,
  "processes": {
    "limit": 1048576,
    "used": 284
  },
  "product_name": "",
  "product_version": "",
  "rabbitmq_version": "3.12.1",
  "raft_data_directory": "/var/lib/rabbitmq/mnesia/rabbit@c75013d4b192/quorum/rabbit@c75013d4b192",
  "release_series_support_status": "supported",
  "run_queue": 1,
  "totals": {
    "connection_count": 0,
    "queue_count": 0,
    "virtual_host_count": 1
  },
  "uptime": 6,
  "vm_memory_calculation_strategy": "rss",
  "vm_memory_high_watermark_limit": 3082623385,
  "vm_memory_high_watermark_setting": {
    "relative": 0.4
  }
}"#;

/// What the Erlang runtime wrote ahead of the document on a GitHub runner, verbatim.
const PREAMBLE: &str = "=ERROR REPORT==== 23-Sep-2026::14:03:05.184639 ===\nfile:path_eval([\"/var/lib/rabbitmq\",\"/home/runner/.config/erlang\"],\".erlang\"): permission denied\n\n";

#[test]
fn parse_reads_a_status_from_an_older_broker() {
    // Act
    let status = RabbitmqctlStatus::parse(OLDER).expect("3.12 is still a RabbitMQ");

    // Assert: a key the newer version does not have is ignored rather than refused, and one
    // it does not carry at all defaults. An upgrade of a box nobody touched must not fail
    // this facet.
    assert_eq!(status.rabbitmq_version, "3.12.1");
    assert!(status.tags.is_empty());
    assert!(status.data_directory.is_some());
}

#[test]
fn parse_reads_past_what_the_erlang_runtime_said_first() {
    // Arrange: the exact bytes a GitHub runner put ahead of the document, which failed every
    // read of this facet in CI while the same version answered cleanly in a container.
    let noisy = format!("{PREAMBLE}{MEASURED}");

    // Act
    let status = RabbitmqctlStatus::parse(&noisy).expect("the document is still in there");

    // Assert
    assert_eq!(status.rabbitmq_version, "4.0.5");
}

#[test]
fn parse_still_refuses_output_with_no_document_in_it_at_all() {
    // Act & Assert: reading past a preamble is not the same as tolerating anything. A usage
    // dump carries no line beginning with a brace, so the failure still names what the tool
    // actually said.
    assert!(RabbitmqctlStatus::parse(&format!("{PREAMBLE}Usage\n\nrabbitmqctl [--node]")).is_err());
}

#[test]
fn a_list_answer_is_a_document_too() {
    // Arrange: every `list_*` command answers with an array rather than an object, and the
    // same runtime report can precede it.
    let noisy = format!("{PREAMBLE}[{{\"name\":\"/\"}}]\n");

    // Act & Assert: a reader that knew only about `{{` would skip the whole answer looking
    // for one. Found by the conformance test, which asks the broker directly and met the
    // noise on its own side.
    assert_eq!(document_in(&noisy).trim(), "[{\"name\":\"/\"}]");
}

#[test]
fn a_reports_own_bracket_does_not_start_the_document() {
    // Arrange: the report's second line is `file:path_eval([...`, which contains a bracket
    // and does not begin with one.
    let noisy = format!("{PREAMBLE}{MEASURED}");

    // Act & Assert: whole lines, rather than hunting for the first bracket anywhere.
    assert!(document_in(&noisy).starts_with('{'));
}

/// A status from a node that has hit a limit, with the alarm exactly as it was measured by
/// raising one: `rabbitmqctl set_vm_memory_high_watermark 0.0001`.
const ALARMED: &str = r#"{"rabbitmq_version":"4.0.5","erlang_version":"Erlang/OTP 27",
  "alarms":[{"node":"rabbit@box","type":"resource_limit","resource":"memory"}],
  "listeners":[{"node":"rabbit@box","port":25672,"protocol":"clustering","interface":"[::]"}]}"#;

#[test]
fn parse_records_an_alarm_the_node_has_raised() {
    // Act
    let status = RabbitmqctlStatus::parse(ALARMED).expect("well formed");

    // Assert: while this is up the node blocks publishing connections, which is the state an
    // operator opens a fingerprint to explain.
    assert_eq!(status.alarms.len(), 1);
    assert_eq!(status.alarms[0].alarm_type, "resource_limit");
    assert_eq!(status.alarms[0].resource, "memory");
}

#[test]
fn an_alarm_is_volatile_because_it_comes_and_goes_with_load() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(ALARMED).expect("well formed"));

    // Assert: measured by raising one and watching it clear. Recorded rather than dropped,
    // because `--include-volatile` is for the operator standing in front of the box, and
    // annotated so two runs of an unchanged host stay byte-identical.
    assert_eq!(
        field(&rendered, "alarms").volatility(),
        Volatility::Volatile
    );
}

#[test]
fn the_nodes_own_name_is_dropped_from_an_alarm() {
    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(ALARMED).expect("well formed"));
    let alarm = &items_of(&field(&rendered, "alarms"))[0];

    // Assert: the report carries `node`, and it is the node this alarm already sits under.
    // Repeating it would be the document arguing with its own keys.
    assert_eq!(keys_of(alarm), ["resource", "type"]);
}

#[test]
fn only_a_node_new_enough_answers_about_feature_flags() {
    // Act & Assert: the subsystem arrived in 3.8.0 — every flag the documentation lists as
    // earliest is from that release — so an older node has no such subcommand.
    let older =
        RabbitmqctlStatus::parse(&MEASURED.replace("4.0.5", "3.7.28")).expect("well formed");
    let boundary =
        RabbitmqctlStatus::parse(&MEASURED.replace("4.0.5", "3.8.0")).expect("well formed");
    let newer = RabbitmqctlStatus::parse(MEASURED).expect("well formed");

    assert!(!older.answers_about_feature_flags());
    assert!(boundary.answers_about_feature_flags());
    assert!(newer.answers_about_feature_flags());
}

#[test]
fn a_version_that_cannot_be_read_is_asked_anyway() {
    // Act & Assert: a node reporting something unparseable is a surprise worth a loud failure
    // rather than a silent omission, so it is asked and the read reports whatever happens.
    let strange = RabbitmqctlStatus::parse(&MEASURED.replace("4.0.5", "chef-special"))
        .expect("a version is text");

    assert!(strange.answers_about_feature_flags());
}

#[test]
fn a_listener_the_node_gave_no_port_for_is_not_recorded() {
    // Arrange
    let portless = r#"{"rabbitmq_version":"4.0.5","erlang_version":"Erlang/OTP 27",
      "listeners":[{"node":"rabbit@box","protocol":"amqp","interface":"[::]"},
                   {"node":"rabbit@box","protocol":"clustering","interface":"[::]","port":25672}]}"#;

    // Act
    let status = RabbitmqctlStatus::parse(portless).expect("well formed");

    // Assert: zero is a port number, so recording one the node never gave would put a socket
    // in the document that does not exist, and a reader could not tell it from a real one.
    assert_eq!(status.listeners.len(), 1);
    assert_eq!(status.listeners[0].port, 25672);
}

#[test]
fn a_directory_the_node_did_not_name_is_absent_rather_than_empty() {
    // Arrange
    let bare = r#"{"rabbitmq_version":"4.0.5","erlang_version":"Erlang/OTP 27"}"#;

    // Act
    let rendered = Observation::from(&RabbitmqctlStatus::parse(bare).expect("well formed"));

    // Assert: an empty string asserts a path of no characters; null says the node did not say.
    assert!(is_null(&field(&rendered, "data_directory")));
    assert!(is_null(&field(&rendered, "operating_system")));
}
