//! One containerd namespace, and what is in it.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::{ContainerdContainer, UnreadableObject};
use crate::collectors::containers::value_objects::ContainerId;

/// The containers of one namespace, keyed by id.
///
/// **Keyed by id rather than by name, unlike docker's containers, because containerd has no
/// names.** A container's id is whatever created it chose: docker uses a 64-character hex
/// string, a kubelet uses one too, and `nerdctl` uses the name the operator typed. There is
/// no second identifier to prefer.
///
/// Two namespaces may hold the same id, which is what makes the namespace the outer key
/// rather than a field on each container.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerdNamespace {
    containers: BTreeMap<ContainerId, ContainerdContainer>,
    unreadable: Vec<UnreadableObject>,
}

impl ContainerdNamespace {
    pub fn new(
        read: impl IntoIterator<Item = (ContainerId, ContainerdContainer)>,
        unreadable: impl IntoIterator<Item = UnreadableObject>,
    ) -> Result<Self, CollectionError> {
        let mut containers = BTreeMap::new();

        for (id, container) in read {
            if containers.insert(id.clone(), container).is_some() {
                return Err(CollectionError::new(format!(
                    "containerd reported the container {:?} twice in one namespace, so the \
                     answer was misread",
                    id.as_str()
                )));
            }
        }

        let mut unreadable: Vec<UnreadableObject> = unreadable.into_iter().collect();
        unreadable.sort();

        Ok(Self {
            containers,
            unreadable,
        })
    }

    pub fn containers(&self) -> &BTreeMap<ContainerId, ContainerdContainer> {
        &self.containers
    }

    pub fn unreadable(&self) -> &[UnreadableObject] {
        &self.unreadable
    }
}

impl From<&ContainerdNamespace> for Observation {
    fn from(namespace: &ContainerdNamespace) -> Self {
        Observation::object([
            (
                "containers",
                Observation::object(
                    namespace
                        .containers()
                        .iter()
                        .map(|(id, container)| (id.as_str(), Observation::from(container))),
                ),
            ),
            (
                // Volatile for the reason docker's list is: a container that came and went
                // between the id list and the read of it is the host changing on its own.
                "unreadable_containers",
                Observation::list(namespace.unreadable().iter().map(Observation::from)).volatile(),
            ),
        ])
    }
}
