//! `GET /_nodes/_local`, narrowed to what this node is configured with.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::collectors::elasticsearch::model::NodeLocal;
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::api_value_of::api_value_of;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

/// Measured by the second domain review on 7.17.24 and 9.2.0: 2.4 KB and 3.3 KB, no warning, the
/// deprecation log unchanged.
const PATH: &str = "/_nodes/_local?flat_settings=true&filter_path=nodes.*.settings,nodes.*.roles,nodes.*.attributes,nodes.*.jvm.input_arguments,nodes.*.jvm.mem.heap_max_in_bytes";

#[derive(Deserialize)]
struct NodesAnswer {
    nodes: BTreeMap<String, LocalAnswer>,
}

#[derive(Deserialize)]
struct LocalAnswer {
    #[serde(default)]
    settings: serde_json::Value,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    attributes: BTreeMap<String, String>,
    #[serde(default)]
    jvm: JvmAnswer,
}

#[derive(Deserialize, Default)]
struct JvmAnswer {
    #[serde(default)]
    input_arguments: Vec<String>,
    #[serde(default)]
    mem: MemoryAnswer,
}

#[derive(Deserialize, Default)]
struct MemoryAnswer {
    heap_max_in_bytes: Option<i64>,
}

/// The asked node's own settings. The answer is keyed by node id, which is not recorded, as the
/// plugins read declines to.
pub fn read_node_local(client: &HttpClient, endpoint: &HttpEndpoint) -> Result<NodeLocal, Unread> {
    let answer: NodesAnswer = read_answer(&client.get(endpoint, PATH)?, PATH)?;
    let count = answer.nodes.len();
    let Some(local) = answer.nodes.into_values().next().filter(|_| count == 1) else {
        return Err(Unread::new(format!(
            "GET /_nodes/_local answered for {count} nodes rather than one node, the one asked"
        )));
    };

    let mut roles = local.roles;
    roles.sort();

    Ok(NodeLocal {
        roles,
        attributes: local.attributes,
        settings: api_value_of(&local.settings),
        jvm_arguments: local.jvm.input_arguments,
        heap_max_bytes: local.jvm.mem.heap_max_in_bytes,
    })
}
