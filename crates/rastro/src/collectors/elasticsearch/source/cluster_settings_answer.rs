//! `GET /_cluster/settings?flat_settings=true`.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::collectors::elasticsearch::model::ClusterSettings;
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::api_value_of::api_value_of;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

/// Flat, so a setting is one key however deeply the node would nest it, and a setting moved
/// between spellings does not read as a change.
const PATH: &str = "/_cluster/settings?flat_settings=true";

#[derive(Deserialize)]
struct ClusterSettingsAnswer {
    #[serde(default)]
    persistent: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    transient: BTreeMap<String, serde_json::Value>,
}

pub fn read_cluster_settings(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
) -> Result<ClusterSettings, Unread> {
    let answer: ClusterSettingsAnswer = read_answer(&client.get(endpoint, PATH)?, PATH)?;
    let converted = |values: BTreeMap<String, serde_json::Value>| {
        values
            .into_iter()
            .map(|(name, value)| (name, api_value_of(&value)))
            .collect()
    };

    Ok(ClusterSettings {
        persistent: converted(answer.persistent),
        transient: converted(answer.transient),
    })
}
