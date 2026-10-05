//! `GET /_ilm/policy`, `GET /_ingest/pipeline` and `GET /_snapshot`.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::collectors::elasticsearch::model::{
    IlmPolicies, IlmPolicy, NamedDefinitions, SnapshotRepositories, SnapshotRepository,
};
use crate::collectors::elasticsearch::source::api_value_of::api_value_of;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::source::{HttpClient, NotFound};
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

const ILM: &str = "/_ilm/policy";
const PIPELINES: &str = "/_ingest/pipeline";
const SNAPSHOTS: &str = "/_snapshot";

#[derive(Deserialize)]
struct IlmPolicyAnswer {
    version: Option<i64>,
    policy: serde_json::Value,
}

#[derive(Deserialize)]
struct SnapshotRepositoryAnswer {
    #[serde(rename = "type")]
    repository_type: String,
    #[serde(default)]
    settings: serde_json::Value,
}

pub fn read_ilm_policies(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
) -> Result<IlmPolicies, Unread> {
    let answer: BTreeMap<String, IlmPolicyAnswer> = read_answer(&client.get(endpoint, ILM)?, ILM)?;

    Ok(IlmPolicies(
        answer
            .into_iter()
            .map(|(name, policy)| {
                (
                    name,
                    IlmPolicy {
                        version: policy.version,
                        policy: api_value_of(&policy.policy),
                    },
                )
            })
            .collect(),
    ))
}

pub fn read_ingest_pipelines(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
) -> Result<NamedDefinitions, Unread> {
    // Measured on 7.17.29 with its built-in pipelines deleted: no pipeline is a 404 of `{}`.
    let body = client.get_where(endpoint, PIPELINES, NotFound::NothingThere)?;
    let answer: BTreeMap<String, serde_json::Value> = read_answer(&body, PIPELINES)?;

    Ok(NamedDefinitions(
        answer
            .into_iter()
            .map(|(name, pipeline)| (name, api_value_of(&pipeline)))
            .collect(),
    ))
}

pub fn read_snapshot_repositories(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
) -> Result<SnapshotRepositories, Unread> {
    let answer: BTreeMap<String, SnapshotRepositoryAnswer> =
        read_answer(&client.get(endpoint, SNAPSHOTS)?, SNAPSHOTS)?;

    Ok(SnapshotRepositories(
        answer
            .into_iter()
            .map(|(name, repository)| {
                (
                    name,
                    SnapshotRepository {
                        repository_type: repository.repository_type,
                        settings: api_value_of(&repository.settings),
                    },
                )
            })
            .collect(),
    ))
}
