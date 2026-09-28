//! The indices, in three requests, none of which reaches a hidden or system index.
//!
//! `expand_wildcards=open,closed` is what keeps them out: `*` then matches no hidden index, and
//! every system index is hidden. Measured on 7.17.24 and 8.15.3, the three requests carry no
//! deprecation warning and change nothing.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::collectors::elasticsearch::model::{IndexEntry, Indices};
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::api_value_of::api_value_of;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{ApiValue, HttpEndpoint, Unread};

const ALIASES: &str = "/*/_alias?expand_wildcards=open,closed";
const SETTINGS: &str = "/*/_settings?flat_settings=true&expand_wildcards=open,closed";
const MAPPINGS: &str = "/*/_mapping?expand_wildcards=open,closed";

const UUID: &str = "index.uuid";
const CREATION_DATE: &str = "index.creation_date";
const PROVIDED_NAME: &str = "index.provided_name";

#[derive(Deserialize)]
struct AliasesOf {
    #[serde(default)]
    aliases: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct SettingsOf {
    settings: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct MappingsOf {
    #[serde(default)]
    mappings: serde_json::Value,
}

pub fn read_indices(client: &HttpClient, endpoint: &HttpEndpoint) -> Result<Indices, Unread> {
    let aliases: BTreeMap<String, AliasesOf> =
        read_answer(&client.get(endpoint, ALIASES)?, ALIASES)?;
    let settings: BTreeMap<String, SettingsOf> =
        read_answer(&client.get(endpoint, SETTINGS)?, SETTINGS)?;
    let mut mappings: BTreeMap<String, MappingsOf> =
        read_answer(&client.get(endpoint, MAPPINGS)?, MAPPINGS)?;

    let aliases_of: BTreeMap<String, Vec<String>> = aliases
        .into_iter()
        .map(|(index, of)| (index, of.aliases.into_keys().collect()))
        .collect();
    let mut indices_behind: BTreeMap<&str, usize> = BTreeMap::new();
    for alias in aliases_of.values().flatten() {
        *indices_behind.entry(alias).or_default() += 1;
    }

    // The settings answer is the list, since every index has settings. An index made or
    // dropped between the three requests is taken as far as they saw it.
    let mut entries = BTreeMap::new();
    for (index, of) in settings {
        let aliases = aliases_of.get(&index).cloned().unwrap_or_default();
        let keyed_by_alias =
            matches!(aliases.as_slice(), [alias] if indices_behind.get(alias.as_str()) == Some(&1));
        let mappings = mappings
            .remove(&index)
            .map(|of| api_value_of(&of.mappings))
            .unwrap_or(ApiValue::Null);
        let entry = entry_of(index, keyed_by_alias, aliases, of.settings, &mappings);
        entries.insert(entry.key().to_owned(), entry);
    }

    Ok(Indices(entries))
}

fn entry_of(
    index: String,
    keyed_by_alias: bool,
    aliases: Vec<String>,
    settings: BTreeMap<String, serde_json::Value>,
    mappings: &ApiValue,
) -> IndexEntry {
    let text_of = |name: &str| {
        settings
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    };
    let per_index: BTreeSet<&str> = [UUID, CREATION_DATE, PROVIDED_NAME].into();

    IndexEntry {
        uuid: text_of(UUID),
        creation_date: text_of(CREATION_DATE),
        settings: settings
            .iter()
            .filter(|(name, _)| !per_index.contains(name.as_str()))
            .map(|(name, value)| (name.clone(), api_value_of(value)))
            .collect(),
        mappings_digest: mappings.digest(),
        index,
        keyed_by_alias,
        aliases,
    }
}
