//! Every network the engine holds, and the ones it would not describe.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::{DockerNetwork, UnreadableObject};
use crate::collectors::containers::value_objects::NetworkName;

/// The networks, keyed by name.
///
/// **Keyed by name, the same as a container's own view of them**, so the two ends of one
/// network are read under one word. The id is a value here for the reason it is a value
/// there: a network destroyed and recreated under the same name is a different network, and
/// the id is what says so.
///
/// The engine's own three — `bridge`, `host` and `none` — are included. Their options are
/// state: the default bridge's `enable_icc` decides whether every container that did not
/// choose a network can reach every other, and nothing else in the document says so.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerNetworks {
    held: BTreeMap<NetworkName, DockerNetwork>,
    unreadable: Vec<UnreadableObject>,
}

impl DockerNetworks {
    pub fn new(
        read: impl IntoIterator<Item = (NetworkName, DockerNetwork)>,
        unreadable: impl IntoIterator<Item = UnreadableObject>,
    ) -> Result<Self, CollectionError> {
        let mut held = BTreeMap::new();

        for (name, network) in read {
            if held.insert(name.clone(), network).is_some() {
                return Err(CollectionError::new(format!(
                    "docker reported the network {:?} twice, so the answer was misread",
                    name.as_str()
                )));
            }
        }

        let mut unreadable: Vec<UnreadableObject> = unreadable.into_iter().collect();
        unreadable.sort();

        Ok(Self { held, unreadable })
    }

    pub fn held(&self) -> &BTreeMap<NetworkName, DockerNetwork> {
        &self.held
    }

    pub fn unreadable(&self) -> &[UnreadableObject] {
        &self.unreadable
    }
}

impl From<&DockerNetworks> for Observation {
    fn from(networks: &DockerNetworks) -> Self {
        Observation::object(
            networks
                .held()
                .iter()
                .map(|(name, network)| (name.as_str(), Observation::from(network))),
        )
    }
}
