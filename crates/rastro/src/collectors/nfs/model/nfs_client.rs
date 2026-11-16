//! The kernel NFS client's own settings.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use super::module_parameters::ModuleParameters;

/// The client half of the facet: the parameters of each client module, `None` where not loaded.
///
/// Only what the rest of the document does not say. The mounts themselves, with the options the
/// kernel actually mounted them with, are the `mounts` facet's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsClient {
    pub parameters: BTreeMap<&'static str, Option<ModuleParameters>>,
}

impl From<&NfsClient> for Observation {
    fn from(client: &NfsClient) -> Self {
        Observation::object([(
            "parameters",
            Observation::object(client.parameters.iter().map(|(module, parameters)| {
                (
                    *module,
                    parameters
                        .as_ref()
                        .map_or_else(Observation::null, Observation::from),
                )
            })),
        )])
    }
}
