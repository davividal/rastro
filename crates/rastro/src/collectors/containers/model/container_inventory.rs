//! Everything on this box that runs containers, and everything they run.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::ContainerEngine;
use crate::collectors::containers::value_objects::EngineFlavour;

/// The facet's two halves: the engines, and the containers they hold.
///
/// **Containers are hoisted out of the engine that runs them, and that is the facet's
/// central arrangement.** A container is a tenant of the box; an engine is the thing that
/// happens to be running it, and an image or a volume is an artefact of that engine's store.
/// So "what is running here" is one subtree a reader can open without knowing which engines
/// exist, and "what is installed here" is its sibling.
///
/// The engine is still the first key under `containers`, because it has to be: `docker/web`
/// and `podman/web` are two different containers with one name, and containerd has no names
/// at all. Which engine runs what is therefore readable from the path as well as from the
/// engine's own entry.
///
/// **One thing deliberately stays with its engine: everything that is not a container.**
/// Images, volumes and networks are the store's, and the losses from reading — a container
/// that vanished mid-read — belong with the account of the read rather than with the
/// containers that survived it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerInventory(BTreeMap<EngineFlavour, ContainerEngine>);

impl ContainerInventory {
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

impl From<&ContainerInventory> for Observation {
    fn from(inventory: &ContainerInventory) -> Self {
        Observation::object([
            (
                "containers",
                Observation::object(
                    inventory
                        .engines()
                        .iter()
                        .map(|(flavour, engine)| (flavour.as_str(), engine.containers())),
                ),
            ),
            (
                "engines",
                Observation::object(
                    inventory
                        .engines()
                        .iter()
                        .map(|(flavour, engine)| (flavour.as_str(), Observation::from(engine))),
                ),
            ),
        ])
    }
}
