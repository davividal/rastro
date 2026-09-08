//! Every port a container listens on, and where each is published.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::PublishedBinding;
use crate::collectors::containers::value_objects::ExposedPort;

/// The port table, keyed by the engine's own `80/tcp` spelling.
///
/// **An empty binding list is the point of the type.** A port the image exposes and nobody
/// published arrives from docker as `"7777/tcp": null`, and that is real state: the container
/// listens on it and only the box can reach it. Dropping such a port would lose the
/// difference between a service that is internal and one that is not there, which is most of
/// what an operator reads a port table for.
///
/// The bindings of one port are sorted, because publishing without naming an address gives
/// one per family and the engine promises no order between them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerPorts(BTreeMap<ExposedPort, Vec<PublishedBinding>>);

impl ContainerPorts {
    pub fn new(
        ports: impl IntoIterator<Item = (ExposedPort, Vec<PublishedBinding>)>,
    ) -> Result<Self, CollectionError> {
        let mut keyed = BTreeMap::new();

        for (port, mut bindings) in ports {
            bindings.sort();
            if keyed.insert(port.clone(), bindings).is_some() {
                return Err(CollectionError::new(format!(
                    "docker reported the port {:?} twice in one container, so the port table \
                     was misread",
                    port.as_key()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn ports(&self) -> &BTreeMap<ExposedPort, Vec<PublishedBinding>> {
        &self.0
    }
}

impl From<&ContainerPorts> for Observation {
    fn from(ports: &ContainerPorts) -> Self {
        Observation::object(ports.ports().iter().map(|(port, bindings)| {
            (
                port.as_key(),
                Observation::list(bindings.iter().map(Observation::from)),
            )
        }))
    }
}
