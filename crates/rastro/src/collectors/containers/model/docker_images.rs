//! Every image the engine holds, and the ones it would not describe.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::containers::model::{DockerImage, UnreadableObject};
use crate::collectors::containers::value_objects::ImageDigest;

/// The images, keyed by id.
///
/// **Keyed by id rather than by tag**, which is the arrangement the diff needs: a tag is
/// not identity and moving one is precisely the event worth catching. An image pulled
/// afresh under an existing tag appears as a new entry while the old one loses its tags,
/// and both halves of that are visible at once.
///
/// An empty map is a legal value: an engine that has pulled nothing holds no images.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerImages {
    held: BTreeMap<ImageDigest, DockerImage>,
    unreadable: Vec<UnreadableObject>,
}

impl DockerImages {
    pub fn new(
        read: impl IntoIterator<Item = (ImageDigest, DockerImage)>,
        unreadable: impl IntoIterator<Item = UnreadableObject>,
    ) -> Result<Self, CollectionError> {
        let mut held = BTreeMap::new();

        for (id, image) in read {
            if held.insert(id.clone(), image).is_some() {
                return Err(CollectionError::new(format!(
                    "docker reported the image {:?} twice, so the answer was misread",
                    id.as_str()
                )));
            }
        }

        let mut unreadable: Vec<UnreadableObject> = unreadable.into_iter().collect();
        unreadable.sort();

        Ok(Self { held, unreadable })
    }

    pub fn held(&self) -> &BTreeMap<ImageDigest, DockerImage> {
        &self.held
    }

    pub fn unreadable(&self) -> &[UnreadableObject] {
        &self.unreadable
    }
}

impl From<&DockerImages> for Observation {
    fn from(images: &DockerImages) -> Self {
        Observation::object(
            images
                .held()
                .iter()
                .map(|(id, image)| (id.as_str(), Observation::from(image))),
        )
    }
}
