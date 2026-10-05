//! One index, as a before-and-after pair should compare it.

use std::collections::BTreeMap;

use rastro_collector::{Observation, Xxh3Digest};

use crate::collectors::elasticsearch::model::node_identity::optional;
use crate::collectors::elasticsearch::value_objects::ApiValue;

/// Every open or closed index that is not hidden, keyed as [`IndexEntry::key`] says.
///
/// Hidden and system indices are left out: they are the node's own, and one of them,
/// `.ds-ilm-history-*`, gains documents while the node sits idle, measured.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Indices(pub BTreeMap<String, IndexEntry>);

/// One index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// The index's own name, which is the rotated part on a box that rebuilds indices.
    pub index: String,

    /// Whether the entry is keyed by its alias, which makes the index name volatile.
    pub keyed_by_alias: bool,

    /// Each alias with its definition: `is_write_index`, a filter, routing. Found by review:
    /// keeping the names alone let a rollover move the write index, or an alias filter change,
    /// read as nothing changed. Keyed, so an alias added in another order is not a change.
    pub aliases: BTreeMap<String, ApiValue>,

    /// Flat settings, less the three that differ for every index made: `index.uuid`,
    /// `index.creation_date` and `index.provided_name`. `index.version.created` is kept, since
    /// it records which release made the index and moves only when the index is rebuilt by one.
    pub settings: BTreeMap<String, ApiValue>,

    /// XXH3-64 of the mappings, not the mappings, which run to thousands of lines on a real
    /// index and would bury every other change. A digest says the schema changed; the
    /// component template the mapping came from, reported whole, says how.
    pub mappings_digest: Xxh3Digest,

    /// Volatile: generated per index, so a rebuild changes it whatever else it does.
    pub uuid: Option<String>,

    /// Volatile, for the same reason, in epoch milliseconds as the node spells it.
    pub creation_date: Option<String>,
}

impl IndexEntry {
    /// The key: the alias where this index is the only one behind exactly one alias, the index
    /// name otherwise.
    ///
    /// An alias over several indices names none of them alone, and an index with several aliases
    /// has no one alias that is its identity, so both keep their names rather than rastro picking.
    pub fn key(&self) -> &str {
        let mut names = self.aliases.keys();
        match (self.keyed_by_alias, names.next(), names.next()) {
            (true, Some(alias), None) => alias,
            _ => &self.index,
        }
    }
}

impl From<&Indices> for Observation {
    fn from(indices: &Indices) -> Self {
        Observation::object(
            indices
                .0
                .iter()
                .map(|(key, entry)| (key.as_str(), Observation::from(entry))),
        )
    }
}

impl From<&IndexEntry> for Observation {
    fn from(entry: &IndexEntry) -> Self {
        let index = Observation::text(&entry.index);

        Observation::object([
            (
                "index",
                match entry.keyed_by_alias {
                    true => index.volatile(),
                    false => index,
                },
            ),
            (
                "aliases",
                Observation::object(
                    entry
                        .aliases
                        .iter()
                        .map(|(name, definition)| (name.as_str(), Observation::from(definition))),
                ),
            ),
            (
                "settings",
                Observation::object(
                    entry
                        .settings
                        .iter()
                        // Each withheld, found by review: a plugin's unfiltered credential can be
                        // an index setting, as it can a cluster one.
                        .map(|(name, value)| (name.as_str(), Observation::from(value).sensitive())),
                ),
            ),
            (
                "mappings_digest",
                Observation::text(entry.mappings_digest.as_str()),
            ),
            ("uuid", optional(entry.uuid.as_deref()).volatile()),
            (
                "creation_date",
                optional(entry.creation_date.as_deref()).volatile(),
            ),
        ])
    }
}
