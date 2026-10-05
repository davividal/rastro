//! The indices, in three requests, none of which reaches a hidden or system index.
//!
//! Two exclusions in the request and one in the answer. `-.*` leaves out every dot-prefixed name,
//! which is where the stack keeps its own indices, system ones included; on 7.17 those are not
//! hidden, and reaching them made the node log and index a deprecation warning. `expand_wildcards=
//! open,closed` leaves out hidden indices. **Not the backing indices of a data stream that is not
//! itself hidden**, measured on 8.15.3 and 9.2.0: the filter is applied to the stream, which then
//! expands to its hidden backing indices. Those are `.ds-` named, so `-.*` now leaves them out
//! too, and an index whose own settings say `index.hidden` is still skipped as a second guard.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::collectors::elasticsearch::model::{IndexEntry, Indices};
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::api_value_of::api_value_of;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{ApiValue, HttpEndpoint, Unread};

/// `*,-.*`: every index whose name does not start with a dot. Found by the second domain review,
/// measured on 7.17.24: there system indices are not hidden, `*` alone matched `.tasks` and
/// `.kibana_*`, and the node answered with a "this request accesses system indices" warning,
/// which it logged and indexed into its deprecation data stream. Excluding dot-prefixed names in
/// the request resolves no system index on any version, so nothing warns.
const ALIASES: &str = "/*,-.*/_alias?expand_wildcards=open,closed";
const SETTINGS: &str = "/*,-.*/_settings?flat_settings=true&expand_wildcards=open,closed";
const MAPPINGS: &str = "/*,-.*/_mapping?expand_wildcards=open,closed";

const UUID: &str = "index.uuid";
const CREATION_DATE: &str = "index.creation_date";
const PROVIDED_NAME: &str = "index.provided_name";

/// What a hidden index carries in its flat settings, a data stream's backing index included.
const HIDDEN: &str = "index.hidden";

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
    let large = client.for_large_answers();
    let aliases: BTreeMap<String, AliasesOf> =
        read_answer(&client.get(endpoint, ALIASES)?, ALIASES)?;
    let settings: BTreeMap<String, SettingsOf> =
        read_answer(&large.get(endpoint, SETTINGS)?, SETTINGS)?;
    let mut mappings: BTreeMap<String, MappingsOf> =
        read_answer(&large.get(endpoint, MAPPINGS)?, MAPPINGS)?;

    let aliases_of: BTreeMap<String, BTreeMap<String, ApiValue>> = aliases
        .into_iter()
        .map(|(index, of)| {
            let definitions = of
                .aliases
                .iter()
                .map(|(name, definition)| (name.clone(), api_value_of(definition)))
                .collect();
            (index, definitions)
        })
        .collect();
    let mut indices_behind: BTreeMap<&str, usize> = BTreeMap::new();
    for alias in aliases_of.values().flat_map(BTreeMap::keys) {
        *indices_behind.entry(alias).or_default() += 1;
    }

    // The settings answer is the list, since every index has settings. An index made or
    // dropped between the three requests is taken as far as they saw it.
    let mut entries = BTreeMap::new();
    for (index, of) in settings {
        if of.settings.get(HIDDEN).and_then(|value| value.as_str()) == Some("true") {
            continue;
        }
        let aliases = aliases_of.get(&index).cloned().unwrap_or_default();
        let keyed_by_alias = aliases.len() == 1
            && aliases
                .keys()
                .all(|alias| indices_behind.get(alias.as_str()) == Some(&1));
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
    aliases: BTreeMap<String, ApiValue>,
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
