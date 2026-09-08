//! Which machine an image was built for.

use rastro_collector::{NonEmptyText, Observation};

/// The platform triple the engine resolved for an image.
///
/// **Worth recording because a multi-architecture tag hides it.** `alpine:latest` is a
/// manifest list, and what a box pulled from it depends on the box; an image whose
/// architecture does not match the host is a container that will not start, and on a
/// mixed fleet that is a real and confusing failure. The variant is the third value
/// because `arm64` alone does not say `v8`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePlatform {
    pub architecture: NonEmptyText,
    pub operating_system: NonEmptyText,
    /// Absent for a platform with no variant, which is most of them.
    pub variant: Option<NonEmptyText>,
}

impl From<&ImagePlatform> for Observation {
    fn from(platform: &ImagePlatform) -> Self {
        Observation::object([
            (
                "architecture",
                Observation::text(platform.architecture.as_str()),
            ),
            (
                "operating_system",
                Observation::text(platform.operating_system.as_str()),
            ),
            (
                "variant",
                match &platform.variant {
                    Some(variant) => Observation::text(variant.as_str()),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
