//! The image behind a container.

use rastro_collector::Observation;

use crate::collectors::containers::value_objects::{ImageDigest, ImageReference};

/// What the container asked for, and what it got.
///
/// **Three values rather than one, because they answer different questions and disagree in
/// useful ways.** The reference is the operator's declaration and is what a compose file or
/// a unit would show. The id is the image the engine actually resolved that reference to, and
/// it is what the container is running. The manifest digest is what a registry would serve
/// for the same reference today.
///
/// A tag repointed upstream, pulled, and the container recreated moves the id while the
/// reference stands still. That is the change a file-hashing tool is silent about, and the
/// reason this facet is not a list of names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerImage {
    pub reference: ImageReference,
    /// The image the engine resolved, which is a digest over its configuration.
    pub id: ImageDigest,
    /// The digest of the manifest the image was pulled as.
    ///
    /// Absent for an image the box built itself and never pushed, which has no manifest in
    /// any registry to be addressed by.
    pub manifest_digest: Option<ImageDigest>,
}

impl From<&ContainerImage> for Observation {
    fn from(image: &ContainerImage) -> Self {
        Observation::object([
            ("id", Observation::from(&image.id)),
            (
                "manifest_digest",
                match &image.manifest_digest {
                    Some(digest) => Observation::from(digest),
                    None => Observation::null(),
                },
            ),
            ("reference", Observation::from(&image.reference)),
        ])
    }
}
