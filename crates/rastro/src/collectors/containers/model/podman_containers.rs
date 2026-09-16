//! Every container a podman service reports.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::PodmanContainer;
use crate::collectors::containers::value_objects::ContainerName;

/// The containers, keyed by name.
///
/// Keyed the way docker's are and for the same reason: a name outlives the id it is minted
/// with, so a recreated container is one entry that changed rather than two that appeared
/// and vanished.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PodmanContainers(BTreeMap<ContainerName, PodmanContainer>);

impl PodmanContainers {
    pub fn new(
        containers: impl IntoIterator<Item = (ContainerName, PodmanContainer)>,
    ) -> Result<Self, CollectionError> {
        let mut keyed = BTreeMap::new();

        for (name, container) in containers {
            if keyed.insert(name.clone(), container).is_some() {
                return Err(CollectionError::new(format!(
                    "podman reported the container {:?} twice, so the answer was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn named(&self) -> &BTreeMap<ContainerName, PodmanContainer> {
        &self.0
    }
}

impl From<&PodmanContainers> for Observation {
    fn from(containers: &PodmanContainers) -> Self {
        Observation::object(
            containers
                .named()
                .iter()
                .map(|(name, container)| (name.as_str(), Observation::from(container))),
        )
    }
}
