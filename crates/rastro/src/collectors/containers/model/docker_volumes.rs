//! Every volume the engine holds, and the ones it would not describe.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::{DockerVolume, UnreadableObject};
use crate::collectors::containers::value_objects::VolumeName;

/// The volumes, keyed by name.
///
/// **These outlive the containers that used them**, which is why they are read in their own
/// right rather than only as seen from a container's mounts: a volume left behind by a
/// container that has been deleted is invisible from every other part of this facet, and it
/// is where a box's data and a box's wasted disk both are.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerVolumes {
    held: BTreeMap<VolumeName, DockerVolume>,
    unreadable: Vec<UnreadableObject>,
}

impl DockerVolumes {
    pub fn new(
        read: impl IntoIterator<Item = (VolumeName, DockerVolume)>,
        unreadable: impl IntoIterator<Item = UnreadableObject>,
    ) -> Result<Self, CollectionError> {
        let mut held = BTreeMap::new();

        for (name, volume) in read {
            if held.insert(name.clone(), volume).is_some() {
                return Err(CollectionError::new(format!(
                    "docker reported the volume {:?} twice, so the answer was misread",
                    name.as_str()
                )));
            }
        }

        let mut unreadable: Vec<UnreadableObject> = unreadable.into_iter().collect();
        unreadable.sort();

        Ok(Self { held, unreadable })
    }

    pub fn held(&self) -> &BTreeMap<VolumeName, DockerVolume> {
        &self.held
    }

    pub fn unreadable(&self) -> &[UnreadableObject] {
        &self.unreadable
    }
}

impl From<&DockerVolumes> for Observation {
    fn from(volumes: &DockerVolumes) -> Self {
        Observation::object(
            volumes
                .held()
                .iter()
                .map(|(name, volume)| (name.as_str(), Observation::from(volume))),
        )
    }
}
