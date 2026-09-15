//! Everything on this box that runs containers, and everything they run.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::ContainerEngine;
use crate::collectors::containers::value_objects::{EngineFlavour, EngineInstance};

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
///
/// **A flavour holds instances rather than one engine**, because one flavour is not one
/// engine: every user on a box can run their own podman with its own store, so the account
/// that owns an engine is the second key on both halves. On an ordinary box that reads
/// `docker/root`, which is one level of ceremony for the case that has one instance and the
/// only arrangement that does not have to call somebody's engine *the* engine.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerInventory(BTreeMap<EngineFlavour, BTreeMap<EngineInstance, ContainerEngine>>);

impl ContainerInventory {
    /// Files each reading under its flavour and the account that owns it, refusing a repeat
    /// of the pair.
    ///
    /// A repeat is not two engines: one account cannot own two of a flavour, since they
    /// would share a store and a socket, so it means the same engine was detected twice.
    pub fn new(
        engines: impl IntoIterator<Item = (EngineInstance, ContainerEngine)>,
    ) -> Result<Self, CollectionError> {
        let mut keyed: BTreeMap<EngineFlavour, BTreeMap<EngineInstance, ContainerEngine>> =
            BTreeMap::new();

        for (instance, engine) in engines {
            let flavour = engine.flavour();
            if keyed
                .entry(flavour)
                .or_default()
                .insert(instance.clone(), engine)
                .is_some()
            {
                return Err(CollectionError::new(format!(
                    "the {} engine belonging to {:?} was read twice, so one engine was \
                     detected twice",
                    flavour.as_str(),
                    instance.as_str()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn engines(&self) -> &BTreeMap<EngineFlavour, BTreeMap<EngineInstance, ContainerEngine>> {
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
                Observation::object(inventory.engines().iter().map(|(flavour, instances)| {
                    (
                        flavour.as_str(),
                        Observation::object(
                            instances
                                .iter()
                                .map(|(instance, engine)| (instance.as_str(), engine.containers())),
                        ),
                    )
                })),
            ),
            (
                "engines",
                Observation::object(inventory.engines().iter().map(|(flavour, instances)| {
                    (
                        flavour.as_str(),
                        Observation::object(instances.iter().map(|(instance, engine)| {
                            (instance.as_str(), Observation::from(engine))
                        })),
                    )
                })),
            ),
        ])
    }
}
