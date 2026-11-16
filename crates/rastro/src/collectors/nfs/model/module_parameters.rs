//! The parameters a loaded module runs with.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::nfs::value_objects::{KernelValue, ParameterName};

/// Every parameter of one loaded module, or why they could not be listed.
///
/// Read whole rather than from a list of names: three of the `nfs` module's parameters are also
/// `fs.nfs.*` sysctls, and a list kept to leave them out would go stale with the next kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleParameters {
    Listed(BTreeMap<ParameterName, KernelValue>),
    Unlisted(String),
}

impl From<&ModuleParameters> for Observation {
    fn from(parameters: &ModuleParameters) -> Self {
        match parameters {
            ModuleParameters::Listed(parameters) => Observation::object(
                parameters
                    .iter()
                    .map(|(name, value)| (name.as_str(), Observation::from(value))),
            ),
            ModuleParameters::Unlisted(reason) => {
                Observation::object([("error", Observation::text(reason.clone()))]).incomplete()
            }
        }
    }
}
