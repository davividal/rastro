//! The namespaces a containerd holds.

use rastro_collector::Observation;

use crate::collectors::containers::value_objects::NamespaceName;

/// Every namespace, sorted.
///
/// A type of its own rather than a bare list, because the namespaces are what everything
/// else containerd holds is read per: its containers and its images are namespaced, and two
/// namespaces may legitimately hold the same id.
///
/// Sorted, because they arrive in the engine's own order and it promises none.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerdNamespaces(Vec<NamespaceName>);

impl ContainerdNamespaces {
    pub fn new(namespaces: impl IntoIterator<Item = NamespaceName>) -> Self {
        let mut namespaces: Vec<NamespaceName> = namespaces.into_iter().collect();
        namespaces.sort();
        namespaces.dedup();

        Self(namespaces)
    }

    pub fn names(&self) -> &[NamespaceName] {
        &self.0
    }
}

impl From<&ContainerdNamespaces> for Observation {
    fn from(namespaces: &ContainerdNamespaces) -> Self {
        Observation::list(namespaces.names().iter().map(Observation::from))
    }
}
