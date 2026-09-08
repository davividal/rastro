//! Everything mounted into one container.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, CollectionError, Observation};

use crate::collectors::containers::model::ContainerMount;

/// The mounts, keyed by the path inside the container.
///
/// **Keyed by destination, not listed in the engine's order.** Measured on docker 26.1.5:
/// two mounts came back in the opposite order from the one they were declared in, so the
/// order is the engine's own and putting it in a document that has to be byte-identical
/// would be trusting something nobody promised. A destination is unique per container, which
/// makes it the honest key.
///
/// **It is also what lets docker's two accounts of a mount be merged.** A `--tmpfs` mount is
/// not in the mount list at all, only in `HostConfig.Tmpfs`, so the two have to be read
/// together; keyed by destination, that is a merge rather than a concatenation. A
/// destination arriving from both is docker contradicting itself, and is refused.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerMounts(BTreeMap<AbsolutePath, ContainerMount>);

impl ContainerMounts {
    pub fn new(
        mounts: impl IntoIterator<Item = (AbsolutePath, ContainerMount)>,
    ) -> Result<Self, CollectionError> {
        let mut keyed = BTreeMap::new();

        for (destination, mount) in mounts {
            if keyed.insert(destination.clone(), mount).is_some() {
                return Err(CollectionError::new(format!(
                    "docker reported two mounts at {:?} in one container, so the mount list \
                     was misread",
                    destination.as_str()
                )));
            }
        }

        Ok(Self(keyed))
    }

    pub fn mounts(&self) -> &BTreeMap<AbsolutePath, ContainerMount> {
        &self.0
    }
}

impl From<&ContainerMounts> for Observation {
    fn from(mounts: &ContainerMounts) -> Self {
        Observation::object(
            mounts
                .mounts()
                .iter()
                .map(|(destination, mount)| (destination.as_str(), Observation::from(mount))),
        )
    }
}
