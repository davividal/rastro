//! `GET /_index_template` and `GET /_component_template`.
//!
//! Composable templates only, which exist from 7.8. Legacy `_template` is not read, as
//! `docs/decisions.md` records.

use serde::Deserialize;

use crate::collectors::elasticsearch::model::NamedDefinitions;
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::api_value_of::api_value_of;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

const INDEX_TEMPLATES: &str = "/_index_template";
const COMPONENT_TEMPLATES: &str = "/_component_template";

#[derive(Deserialize)]
struct IndexTemplatesAnswer {
    index_templates: Vec<IndexTemplateEntry>,
}

#[derive(Deserialize)]
struct IndexTemplateEntry {
    name: String,
    index_template: serde_json::Value,
}

#[derive(Deserialize)]
struct ComponentTemplatesAnswer {
    component_templates: Vec<ComponentTemplateEntry>,
}

#[derive(Deserialize)]
struct ComponentTemplateEntry {
    name: String,
    component_template: serde_json::Value,
}

pub fn read_index_templates(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
) -> Result<NamedDefinitions, Unread> {
    let answer: IndexTemplatesAnswer =
        read_answer(&client.get(endpoint, INDEX_TEMPLATES)?, INDEX_TEMPLATES)?;

    Ok(NamedDefinitions(
        answer
            .index_templates
            .into_iter()
            .map(|entry| (entry.name, api_value_of(&entry.index_template)))
            .collect(),
    ))
}

pub fn read_component_templates(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
) -> Result<NamedDefinitions, Unread> {
    let answer: ComponentTemplatesAnswer = read_answer(
        &client.get(endpoint, COMPONENT_TEMPLATES)?,
        COMPONENT_TEMPLATES,
    )?;

    Ok(NamedDefinitions(
        answer
            .component_templates
            .into_iter()
            .map(|entry| (entry.name, api_value_of(&entry.component_template)))
            .collect(),
    ))
}
