//! `docker image inspect`: docker's spelling of one image.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::{ByteSize, CollectionError, NonEmptyText};

use crate::collectors::containers::model::{DockerImage, ImagePlatform};
use crate::collectors::containers::value_objects::{
    EngineInstant, ImageDigest, ImageReference, LabelName,
};

/// An image as docker describes it, kept apart from rastro's meaning.
///
/// Only the fields this facet reads are declared. What is left out on purpose: the image's
/// own `Config`, which is the container defaults every container running it already reports
/// resolved; `RootFS.Layers`, which is content the id already addresses; and
/// `Metadata.LastTagTime`, which moves when somebody re-tags rather than when the image
/// changes.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerImageDocument {
    #[serde(rename = "Id")]
    id: String,
    /// Empty for a dangling image, which docker otherwise prints as `<none>:<none>`.
    #[serde(rename = "RepoTags", default)]
    tags: Vec<String>,
    /// Empty for an image the box built and never pushed.
    #[serde(rename = "RepoDigests", default)]
    registry_digests: Vec<String>,
    #[serde(rename = "Created")]
    created: String,
    #[serde(rename = "Size")]
    size: i64,
    #[serde(rename = "Architecture")]
    architecture: String,
    #[serde(rename = "Os")]
    operating_system: String,
    #[serde(rename = "Variant", default)]
    variant: String,
    /// Empty on a buildkit image, which records no parent chain.
    #[serde(rename = "Parent", default)]
    parent: String,
    #[serde(rename = "Config", default)]
    config: Option<ConfigHalf>,
}

/// The image's container defaults, read for the labels alone.
#[derive(Debug, Clone, Deserialize)]
struct ConfigHalf {
    /// Null on an image with none, which `default` covers either way.
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
}

impl DockerImageDocument {
    /// Translates docker's document into rastro's model, keyed by the id it sits under.
    pub fn to_image(&self) -> Result<(ImageDigest, DockerImage), CollectionError> {
        let mut tags = references(&self.tags, "image tag")?;
        tags.sort();
        let mut registry_digests = references(&self.registry_digests, "registry digest")?;
        registry_digests.sort();

        let mut labels = BTreeMap::new();
        for (name, value) in self
            .config
            .as_ref()
            .and_then(|config| config.labels.as_ref())
            .into_iter()
            .flatten()
        {
            labels.insert(LabelName::new(name.clone())?, value.clone());
        }

        let image = DockerImage {
            tags,
            registry_digests,
            created: EngineInstant::new(self.created.clone())?,
            size: size_of(self.size)?,
            platform: ImagePlatform {
                architecture: NonEmptyText::new(self.architecture.clone(), "architecture")?,
                operating_system: NonEmptyText::new(
                    self.operating_system.clone(),
                    "operating system",
                )?,
                variant: NonEmptyText::new(self.variant.clone(), "platform variant").ok(),
            },
            labels,
            parent: ImageDigest::new(self.parent.clone()).ok(),
        };

        Ok((ImageDigest::new(self.id.clone())?, image))
    }
}

/// docker's references as rastro's, refusing one it left blank.
fn references(
    reported: &[String],
    kind: &'static str,
) -> Result<Vec<ImageReference>, CollectionError> {
    let mut references = Vec::new();

    for reference in reported {
        references.push(ImageReference::new(reference.clone()).map_err(|failure| {
            CollectionError::new(format!("docker reported an unreadable {kind}: {failure}"))
        })?);
    }

    Ok(references)
}

/// The image's size, refusing a negative figure rather than recording one.
///
/// An image has a size, always: unlike a limit, there is no reading of a missing one, so
/// this fails where a limit would have been absent.
fn size_of(reported: i64) -> Result<ByteSize, CollectionError> {
    let bytes = u64::try_from(reported).map_err(|_| {
        CollectionError::new(format!(
            "docker reported the image size {reported}, and an image cannot be a negative \
             number of bytes"
        ))
    })?;

    ByteSize::new(bytes, "image size")
}
