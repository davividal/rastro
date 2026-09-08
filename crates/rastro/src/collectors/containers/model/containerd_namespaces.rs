//! The namespaces a containerd holds, and what is in each.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::ContainerdNamespace;
use crate::collectors::containers::value_objects::NamespaceName;

/// Every namespace, keyed by name.
///
/// **The namespace is the outer key rather than a field on each container**, because it is
/// containerd's tenancy boundary: two namespaces may hold containers with the same id, and
/// nothing in one is visible from another. Which ones exist also says who is using the
/// engine — `moby` is docker's, `k8s.io` is a kubelet's, `default` is what `nerdctl` uses —
/// so an empty namespace is still worth its key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerdNamespaces(BTreeMap<NamespaceName, ContainerdNamespace>);

impl ContainerdNamespaces {
    pub fn new(
        namespaces: impl IntoIterator<Item = (NamespaceName, ContainerdNamespace)>,
    ) -> Result<Self, CollectionError> {
        let mut keyed = BTreeMap::new();

        for (name, namespace) in namespaces {
            if keyed.insert(name.clone(), namespace).is_some() {
                return Err(CollectionError::new(format!(
                    "containerd reported the namespace {:?} twice, so the answer was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn namespaces(&self) -> &BTreeMap<NamespaceName, ContainerdNamespace> {
        &self.0
    }
}

impl From<&ContainerdNamespaces> for Observation {
    fn from(namespaces: &ContainerdNamespaces) -> Self {
        Observation::object(
            namespaces
                .namespaces()
                .iter()
                .map(|(name, namespace)| (name.as_str(), Observation::from(namespace))),
        )
    }
}
