//! Every container docker has, and the ones it would not describe.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::containers::model::{DockerContainer, UnreadableContainer};
use crate::collectors::containers::value_objects::ContainerName;

/// The containers, keyed by name, beside the losses from reading them.
///
/// **A `BTreeMap`, so no map iteration order reaches the document**, and a name may appear
/// once, which the type enforces rather than trusts: docker holds one container per name, so
/// a repeat means rastro misread the answer, and keeping the last of two would drop a
/// container from a document claiming to be complete.
///
/// An empty map is a legal and meaningful value: a running engine with nothing on it is a
/// real state, and a different one from an engine that could not be asked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerContainers {
    named: BTreeMap<ContainerName, DockerContainer>,
    unreadable: Vec<UnreadableContainer>,
}

impl DockerContainers {
    pub fn new(
        read: impl IntoIterator<Item = (ContainerName, DockerContainer)>,
        unreadable: impl IntoIterator<Item = UnreadableContainer>,
    ) -> Result<Self, CollectionError> {
        let mut named = BTreeMap::new();

        for (name, container) in read {
            if named.insert(name.clone(), container).is_some() {
                return Err(CollectionError::new(format!(
                    "docker reported the container {:?} twice, so the answer was misread",
                    name.as_str()
                )));
            }
        }

        let mut unreadable: Vec<UnreadableContainer> = unreadable.into_iter().collect();
        // Sorted, because the id list arrives newest-first and a container created between
        // two runs would otherwise reorder the ones already in the document.
        unreadable.sort();

        Ok(Self { named, unreadable })
    }

    pub fn named(&self) -> &BTreeMap<ContainerName, DockerContainer> {
        &self.named
    }

    pub fn unreadable(&self) -> &[UnreadableContainer] {
        &self.unreadable
    }
}
