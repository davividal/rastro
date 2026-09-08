//! One file a unit reads its environment from.

use rastro_collector::{AbsolutePath, CollectionError, Observation};

/// An `EnvironmentFile=` as `systemctl show` reports it.
///
/// **This is the half of a unit's environment that `Environment=` cannot show.** systemd
/// opens these at exec time rather than at load time, so nothing they set appears on the
/// `Environment=` line — measured against systemd 257. A service whose whole configuration
/// lives in one of these declares an empty environment and runs with a full one, and the
/// only thing standing between a reader and that misreading is this list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentFile {
    pub path: AbsolutePath,
    /// Whether systemd starts the unit anyway when the file is not there.
    ///
    /// **systemd's own word, kept as systemd spells it**, for what the unit file writes as
    /// the `-` prefix in `EnvironmentFile=-/path`. The double negative is unlovely and it is
    /// the vocabulary of the tool being quoted, which is the spelling a reader can look up.
    ///
    /// The distinction is behaviour rather than bookkeeping: a file the unit requires that
    /// did not survive a migration stops the service, and one marked this way is designed
    /// for exactly that absence.
    pub ignore_errors: bool,
}

impl EnvironmentFile {
    pub fn new(path: impl Into<String>, ignore_errors: bool) -> Result<Self, CollectionError> {
        Ok(Self {
            path: AbsolutePath::new(path, "unit environment file")?,
            ignore_errors,
        })
    }
}

impl From<&EnvironmentFile> for Observation {
    fn from(file: &EnvironmentFile) -> Self {
        Observation::object([
            ("ignore_errors", Observation::boolean(file.ignore_errors)),
            ("path", Observation::text(file.path.as_str())),
        ])
    }
}
