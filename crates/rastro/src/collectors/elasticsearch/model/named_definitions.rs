//! Definitions a node keeps by name: templates, policies, pipelines.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::elasticsearch::value_objects::ApiValue;

/// Every definition of one kind, keyed by its name.
///
/// Keyed rather than listed, because the node lists them in an order of its own that nothing
/// promises to keep across a restart, and a name is what an operator changes one by.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamedDefinitions(pub BTreeMap<String, ApiValue>);

impl From<&NamedDefinitions> for Observation {
    fn from(definitions: &NamedDefinitions) -> Self {
        Observation::object(
            definitions
                .0
                .iter()
                .map(|(name, definition)| (name.as_str(), Observation::from(definition))),
        )
    }
}
