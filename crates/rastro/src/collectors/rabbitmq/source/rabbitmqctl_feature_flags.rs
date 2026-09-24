//! The `rabbitmqctl list_feature_flags --formatter json` interface.
//!
//! **Why a third read of the node is worth its 300 ms.** A feature flag gates a change to the
//! wire format or to how a node stores things, and enabling one is **irreversible**: a
//! cluster that has enabled `khepri_db` cannot go back, and a node that has not cannot join
//! one that has. So the set of enabled flags decides which versions a box can be upgraded to
//! and which nodes it can be clustered with, and none of it is visible in the configuration
//! or in the package version.
//!
//! Measured on 4.0.5: 23 flags, each `{"name": …, "state": …}`, with `khepri_db` the one
//! disabled by default.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::CollectionError;

use super::json_document::read_document;

/// One flag, as the node lists it.
#[derive(Debug, Deserialize)]
struct FlagDocument {
    name: Option<String>,
    state: Option<String>,
}

/// What a node has enabled.
pub struct RabbitmqctlFeatureFlags;

impl RabbitmqctlFeatureFlags {
    /// Reads the flag list into names and their states.
    ///
    /// A map rather than a list: a flag's name is unique on a node and is what a reader looks
    /// up, and the document's key order then does the sorting.
    pub fn parse(output: &str) -> Result<BTreeMap<String, String>, CollectionError> {
        let rows: Vec<FlagDocument> = read_document(output).map_err(|failure| {
            CollectionError::new(format!(
                "rabbitmqctl list_feature_flags did not answer with a JSON document, so \
                     this node's feature flags could not be read: {failure}"
            ))
        })?;

        Ok(rows
            .into_iter()
            .filter_map(|flag| Some((flag.name?, flag.state.unwrap_or_default())))
            .collect())
    }
}
