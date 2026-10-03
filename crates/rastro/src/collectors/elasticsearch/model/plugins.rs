//! The plugins a node runs.

use std::collections::BTreeMap;

use rastro_collector::Observation;

/// Installed plugins by name, each with its version.
///
/// Modules are not listed: they ship inside the build the facet already names by version and
/// hash, so they change exactly when that does. A plugin is installed separately, and one added
/// or removed changes which repository types, analysers and field types the node accepts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plugins(pub BTreeMap<String, String>);

impl From<&Plugins> for Observation {
    fn from(plugins: &Plugins) -> Self {
        Observation::object(
            plugins
                .0
                .iter()
                .map(|(name, version)| (name.as_str(), Observation::text(version))),
        )
    }
}
