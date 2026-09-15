//! One volume the engine holds.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::containers::value_objects::{EngineInstant, LabelName};

/// A volume as rastro means it.
///
/// **The driver options are the reason this is worth a read of its own.** A `local` volume
/// carrying `type=tmpfs` is not durably at the path its mountpoint names, and an NFS volume's
/// options name the server the data actually lives on. A facet that recorded the mountpoint
/// alone would describe the wrong place with confidence, which is worse than describing
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerVolume {
    pub driver: NonEmptyText,
    /// Where the engine mounts it from on this box.
    pub mountpoint: AbsolutePath,
    pub created: EngineInstant,
    /// `local`, or `global` for a volume a swarm manages.
    pub scope: NonEmptyText,
    pub labels: BTreeMap<LabelName, String>,
    /// What the driver was given: for `local`, the `type`, `device` and `o` of a mount.
    pub options: BTreeMap<NonEmptyText, String>,
}

impl From<&DockerVolume> for Observation {
    fn from(volume: &DockerVolume) -> Self {
        Observation::object([
            ("created", Observation::from(&volume.created)),
            ("driver", Observation::text(volume.driver.as_str())),
            (
                "labels",
                Observation::object(
                    volume
                        .labels
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            ("mountpoint", Observation::text(volume.mountpoint.as_str())),
            (
                "options",
                Observation::object(
                    volume
                        .options
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value))),
                ),
            ),
            ("scope", Observation::text(volume.scope.as_str())),
        ])
    }
}
