//! One thing mounted into a container.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::containers::value_objects::MountKind;

/// A mount as rastro means it, whichever of docker's two accounts it came from.
///
/// The fields that are optional are optional because the kinds genuinely differ, not to
/// paper over a read: only a volume has a name and a driver, only a bind and a volume have a
/// source on the host, and only a tmpfs carries the option string docker keeps for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerMount {
    pub kind: MountKind,
    /// The volume's name, absent for every other kind.
    pub name: Option<NonEmptyText>,
    /// Where it comes from on the host: a bind's own path, or the directory the engine keeps
    /// a volume in. Absent for a tmpfs, which comes from nowhere.
    pub source: Option<AbsolutePath>,
    /// The volume driver, absent for every other kind.
    pub driver: Option<NonEmptyText>,
    pub writable: bool,
    /// How a mount made on the host afterwards propagates inside, for the kinds that have
    /// one.
    pub propagation: Option<NonEmptyText>,
    /// The raw option string, which only a tmpfs has: `rw,size=64m`.
    ///
    /// Kept as one string rather than split into pairs, for the reason the mount options in
    /// `/proc/mounts` are: splitting on every comma corrupts any option whose value holds
    /// one, and the string is what the operator wrote.
    pub options: Option<NonEmptyText>,
}

impl From<&ContainerMount> for Observation {
    fn from(mount: &ContainerMount) -> Self {
        Observation::object([
            ("driver", optional(mount.driver.as_ref())),
            ("kind", Observation::from(&mount.kind)),
            ("name", optional(mount.name.as_ref())),
            ("options", optional(mount.options.as_ref())),
            ("propagation", optional(mount.propagation.as_ref())),
            (
                "source",
                match &mount.source {
                    Some(source) => Observation::text(source.as_str()),
                    None => Observation::null(),
                },
            ),
            ("writable", Observation::boolean(mount.writable)),
        ])
    }
}

fn optional(value: Option<&NonEmptyText>) -> Observation {
    match value {
        Some(value) => Observation::text(value.as_str()),
        None => Observation::null(),
    }
}
