//! Every container engine on the box.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::ContainerEngine;
use crate::collectors::containers::value_objects::EngineFlavour;

/// The engines, keyed by flavour.
///
/// **A `BTreeMap`, so no map iteration order can reach the document**, and one entry per
/// flavour, which is a rule the type enforces rather than trusts: a second reading of the
/// same flavour means rastro detected one engine twice and would silently keep whichever
/// arrived last.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerEngines(BTreeMap<EngineFlavour, ContainerEngine>);

impl ContainerEngines {
    /// Files each reading under its flavour, refusing a repeat.
    pub fn new(
        engines: impl IntoIterator<Item = ContainerEngine>,
    ) -> Result<Self, CollectionError> {
        let mut keyed = BTreeMap::new();

        for engine in engines {
            let flavour = engine.flavour();
            if keyed.insert(flavour, engine).is_some() {
                return Err(CollectionError::new(format!(
                    "the {} engine was read twice, so one engine was detected twice",
                    flavour.as_str()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn engines(&self) -> &BTreeMap<EngineFlavour, ContainerEngine> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&ContainerEngines> for Observation {
    fn from(engines: &ContainerEngines) -> Self {
        Observation::object(
            engines
                .engines()
                .iter()
                .map(|(flavour, engine)| (flavour.as_str(), Observation::from(engine))),
        )
    }
}
