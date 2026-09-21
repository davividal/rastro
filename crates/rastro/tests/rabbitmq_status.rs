//! The node's own account of itself, parsed from a status somebody captured.
//!
//! The fixture is verbatim from `rabbitmqctl status --formatter json` on the box the
//! footprint was measured on: RabbitMQ 4.0.5 on Debian 13, Erlang 27. Kept whole, volatile
//! halves included, because half of what this parse has to get right is what it leaves out.

use rastro::collectors::rabbitmq::RabbitmqctlStatus;
use rastro_collector::Observation;

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
