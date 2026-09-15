//! Every network a container is attached to.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::ContainerNetwork;
use crate::collectors::containers::value_objects::NetworkName;

/// The networks, keyed by name.
///
/// A container on two networks is on two networks, and both ends are recorded: which
/// networks a container can reach is as much of its state as which ports it publishes, and a
/// container quietly added to a second network is the change that makes a service reachable
/// from somewhere new.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerNetworks(BTreeMap<NetworkName, ContainerNetwork>);

impl ContainerNetworks {
    pub fn new(
        networks: impl IntoIterator<Item = (NetworkName, ContainerNetwork)>,
    ) -> Result<Self, CollectionError> {
        let mut keyed = BTreeMap::new();

        for (name, network) in networks {
            if keyed.insert(name.clone(), network).is_some() {
                return Err(CollectionError::new(format!(
                    "docker attached one container to the network {:?} twice, so the answer \
                     was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn networks(&self) -> &BTreeMap<NetworkName, ContainerNetwork> {
        &self.0
    }
}

impl From<&ContainerNetworks> for Observation {
    fn from(networks: &ContainerNetworks) -> Self {
        Observation::object(
            networks
                .networks()
                .iter()
                .map(|(name, network)| (name.as_str(), Observation::from(network))),
        )
    }
}
