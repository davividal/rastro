//! `GET /_nodes/_local/plugins`.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::collectors::elasticsearch::model::Plugins;
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

const PATH: &str = "/_nodes/_local/plugins";

#[derive(Deserialize)]
struct NodesAnswer {
    nodes: BTreeMap<String, NodePlugins>,
}

#[derive(Deserialize)]
struct NodePlugins {
    #[serde(default)]
    plugins: Vec<PluginAnswer>,
}

#[derive(Deserialize)]
struct PluginAnswer {
    name: String,
    version: String,
}

/// The asked node's plugins. The answer is keyed by node id, which is not recorded: it is
/// persisted in the data directory and says nothing a plugin list needs.
pub fn read_plugins(client: &HttpClient, endpoint: &HttpEndpoint) -> Result<Plugins, Unread> {
    let answer: NodesAnswer = read_answer(&client.get(endpoint, PATH)?, PATH)?;
    let count = answer.nodes.len();
    let Some(node) = answer.nodes.into_values().next().filter(|_| count == 1) else {
        return Err(Unread::new(format!(
            "GET {PATH} answered for {count} nodes rather than one node, the one asked"
        )));
    };

    Ok(Plugins(
        node.plugins
            .into_iter()
            .map(|plugin| (plugin.name, plugin.version))
            .collect(),
    ))
}
