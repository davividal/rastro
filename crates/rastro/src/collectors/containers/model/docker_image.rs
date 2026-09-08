//! One image the engine holds.

use std::collections::BTreeMap;

use rastro_collector::{ByteSize, Observation};

use crate::collectors::containers::model::ImagePlatform;
use crate::collectors::containers::value_objects::{
    EngineInstant, ImageDigest, ImageReference, LabelName,
};

/// An image as rastro means it: what it is, where it came from, and what it costs.
///
/// **Keyed by its own id elsewhere, and the tags are values here.** That is the opposite
/// arrangement from containers, and for the opposite reason: a container's name outlives
/// its id, while an image's tags are the thing that moves. `nginx:1.29` repointed at a
/// rebuilt image leaves the old image on the box with no tags and gives the new one the
/// tag, and only an id-keyed table shows both halves of that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerImage {
    /// Every tag pointing at this image, sorted. Empty for a dangling image, which is a
    /// real state and usually an accident worth seeing.
    pub tags: Vec<ImageReference>,
    /// The registry digests this image is addressable by, sorted. Empty for an image the
    /// box built and never pushed.
    pub registry_digests: Vec<ImageReference>,
    pub created: EngineInstant,
    pub size: ByteSize,
    pub platform: ImagePlatform,
    /// The image's own labels, which is where a build writes its provenance:
    /// `org.opencontainers.image.revision` names the commit it was built from.
    pub labels: BTreeMap<LabelName, String>,
    /// The image this one was built on, where the engine still knows it.
    pub parent: Option<ImageDigest>,
}

impl From<&DockerImage> for Observation {
    fn from(image: &DockerImage) -> Self {
        Observation::object([
            ("created", Observation::from(&image.created)),
            (
                "labels",
                Observation::object(
                    image
                        .labels
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            (
                "parent",
                match &image.parent {
                    Some(parent) => Observation::from(parent),
                    None => Observation::null(),
                },
            ),
            ("platform", Observation::from(&image.platform)),
            (
                "registry_digests",
                Observation::list(image.registry_digests.iter().map(Observation::from)),
            ),
            ("size_bytes", Observation::integer(image.size.bytes())),
            (
                "tags",
                Observation::list(image.tags.iter().map(Observation::from)),
            ),
        ])
    }
}
