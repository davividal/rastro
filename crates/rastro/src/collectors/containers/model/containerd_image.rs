//! One image containerd holds.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::value_objects::ImageDigest;

/// An image as containerd describes it, which is less than docker's account and honestly so.
///
/// **No size, and that is a decision rather than a gap.** `ctr images ls` prints `3.9 MiB`,
/// a rounded human string, and containerd offers no `images info` to ask for bytes. A
/// rounding in a diffable document changes when the formatting does and not when the image
/// does, so it is left out; docker's own image entry carries real bytes for the images it
/// holds.
///
/// **No labels either, for the same kind of reason.** The only form `ctr` prints is one
/// comma-joined cell, and a label value may itself hold a comma, so splitting it would
/// corrupt values rather than read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerdImage {
    /// The manifest's media type, which says whether this is a single image or an index of
    /// one per platform.
    pub media_type: NonEmptyText,
    pub digest: ImageDigest,
    /// The platforms the manifest carries, sorted. Which ones an image has decides whether
    /// it can run on this box at all.
    pub platforms: Vec<NonEmptyText>,
}

impl From<&ContainerdImage> for Observation {
    fn from(image: &ContainerdImage) -> Self {
        Observation::object([
            ("digest", Observation::from(&image.digest)),
            ("media_type", Observation::text(image.media_type.as_str())),
            (
                "platforms",
                Observation::list(
                    image
                        .platforms
                        .iter()
                        .map(|platform| Observation::text(platform.as_str())),
                ),
            ),
        ])
    }
}
