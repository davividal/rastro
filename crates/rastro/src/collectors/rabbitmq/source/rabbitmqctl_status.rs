//! The `rabbitmqctl status --formatter json` interface.
//!
//! The node's own account of itself, and the one read that says where its files are: the
//! data directory the walk seals, the configuration files it actually read, and where its
//! log goes. JSON because the CLI offers it, which is the unambiguous source rather than the
//! convenient one: the plain-text form of this command is a human-facing report with
//! indentation and units in it.
//!
//! **The volatile half of the document is not deserialised at all.** `memory`, `uptime`,
//! `pid`, `run_queue`, `processes`, `file_descriptors`, `disk_free` and `totals` have no
//! field here, so no later mistake in the model or the renderer can put them in a document.

use serde::Deserialize;

use rastro_collector::CollectionError;

use super::json_document::document_in;

use crate::collectors::rabbitmq::model::{Alarm, Listener, NodeStatus};

/// The subset of `status` rastro reads, spelled as RabbitMQ spells it.
///
/// Unknown fields are ignored by design: RabbitMQ adds keys between releases, and a read
/// that refused an unfamiliar one would fail the facet on the next upgrade of a box nobody
/// changed.
#[derive(Debug, Deserialize)]
struct StatusDocument {
    rabbitmq_version: Option<String>,
    erlang_version: Option<String>,
    crypto_lib_version: Option<String>,
    product_name: Option<String>,
    product_version: Option<String>,
    os: Option<String>,
    data_directory: Option<String>,
    raft_data_directory: Option<String>,
    #[serde(default)]
    config_files: Vec<String>,
    #[serde(default)]
    log_files: Vec<String>,
    enabled_plugin_file: Option<String>,
    #[serde(default)]
    active_plugins: Vec<String>,
    #[serde(default)]
    listeners: Vec<ListenerDocument>,
    #[serde(default)]
    alarms: Vec<AlarmDocument>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    is_under_maintenance: bool,
    net_ticktime: Option<i64>,
    vm_memory_high_watermark_limit: Option<i64>,
    disk_free_limit: Option<i64>,
}

/// An alarm as the node reports it, measured by raising one:
/// `{"node": "rabbit@box", "type": "resource_limit", "resource": "memory"}`.
#[derive(Debug, Deserialize)]
struct AlarmDocument {
    #[serde(rename = "type")]
    alarm_type: Option<String>,
    resource: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListenerDocument {
    node: Option<String>,
    protocol: Option<String>,
    interface: Option<String>,
    port: Option<u16>,
    purpose: Option<String>,
}

/// What a node says about itself.
pub struct RabbitmqctlStatus;

impl RabbitmqctlStatus {
    /// Reads one node's status document.
    ///
    /// **A document with no RabbitMQ version is refused, and that is a safety check rather
    /// than a formality.** The register names every Erlang node on the box, RabbitMQ's and
    /// anybody else's, so an answer without a version is evidence that whatever replied is
    /// not a broker. Recording it would put another application's state under this facet's
    /// name.
    pub fn parse(output: &str) -> Result<NodeStatus, CollectionError> {
        let document: StatusDocument =
            serde_json::from_str(document_in(output)).map_err(|failure| {
                CollectionError::new(format!(
                    "rabbitmqctl status did not answer with a JSON document, so nothing about \
                 this node could be read: {failure}"
                ))
            })?;

        let rabbitmq_version = required(document.rabbitmq_version, "rabbitmq_version")?;
        let erlang_version = required(document.erlang_version, "erlang_version")?;

        Ok(NodeStatus {
            reported_name: document
                .listeners
                .iter()
                .find_map(|listener| listener.node.clone()),
            rabbitmq_version,
            erlang_version,
            crypto_library_version: stated(document.crypto_lib_version),
            product_name: stated(document.product_name),
            product_version: stated(document.product_version),
            operating_system: document.os.unwrap_or_default(),
            data_directory: document.data_directory.unwrap_or_default(),
            raft_data_directory: stated(document.raft_data_directory),
            configuration_files: document.config_files,
            log_destinations: document.log_files,
            enabled_plugins_file: stated(document.enabled_plugin_file),
            active_plugins: document.active_plugins,
            listeners: document.listeners.iter().map(listener_of).collect(),
            alarms: document
                .alarms
                .iter()
                .map(|alarm| Alarm {
                    alarm_type: alarm.alarm_type.clone().unwrap_or_default(),
                    resource: alarm.resource.clone().unwrap_or_default(),
                })
                .collect(),
            tags: document.tags,
            under_maintenance: document.is_under_maintenance,
            net_tick_seconds: document.net_ticktime,
            memory_high_watermark_limit: document.vm_memory_high_watermark_limit,
            disk_free_limit: document.disk_free_limit,
        })
    }
}

/// A field without which the answer is not a RabbitMQ node's.
fn required(value: Option<String>, field: &str) -> Result<String, CollectionError> {
    match value.filter(|value| !value.is_empty()) {
        Some(value) => Ok(value),
        None => Err(CollectionError::new(format!(
            "a status document carrying no {field:?} did not come from a RabbitMQ node, so \
             nothing in it can be recorded as one's state"
        ))),
    }
}

/// What the node actually stated, mapping its empty string to absence.
///
/// The commercial build's product fields print as `""` on the open-source one, and an empty
/// string in a document asserts a value the node never gave.
fn stated(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn listener_of(document: &ListenerDocument) -> Listener {
    Listener {
        protocol: document.protocol.clone().unwrap_or_default(),
        interface: document.interface.clone().unwrap_or_default(),
        port: document.port.unwrap_or_default(),
        purpose: document.purpose.clone(),
    }
}
