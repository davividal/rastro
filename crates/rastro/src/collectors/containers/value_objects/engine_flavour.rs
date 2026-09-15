//! Which container engine this is.

/// The engines rastro can read, and the key each one's state sits under.
///
/// **Keyed by flavour rather than by anything the operator chose**, because a box runs at
/// most one of each, and two of them legitimately sit side by side: docker's daemon runs
/// containerd underneath itself, so a docker box has both, reporting the same containers at
/// two different levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EngineFlavour {
    Containerd,
    Docker,
    Podman,
}

impl EngineFlavour {
    /// Every flavour, which is what detection walks.
    ///
    /// The same hazard [`crate::collectors::packages::PackageManager::ALL`] documents: a
    /// flavour added here and nowhere else is an engine rastro claims to know and never
    /// looks for. The exhaustive match in `EngineSource::detect` is what stops that
    /// compiling.
    pub const ALL: [Self; 3] = [Self::Containerd, Self::Docker, Self::Podman];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Containerd => "containerd",
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}
