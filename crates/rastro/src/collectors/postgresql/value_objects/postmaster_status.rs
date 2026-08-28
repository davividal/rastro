//! What the running postmaster says it is doing, from `postmaster.pid`.

use rastro_collector::CollectionError;

/// The `PM_STATUS` line of `postmaster.pid`.
///
/// **`standby` is the one worth the read.** A streaming standby with `hot_standby = off` is
/// up and deliberately refusing connections, which a psql attempt cannot tell from a broken
/// cluster: both fail to connect. The pid file says which, so a cluster behaving exactly as
/// configured stops reading as an error. The other states round out the picture: `starting`
/// and `stopping` are transitions, `ready` is a primary accepting connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PostmasterStatus {
    Starting,
    Stopping,
    Ready,
    Standby,
}

impl PostmasterStatus {
    /// Reads the word `postmaster.pid` prints, trailing padding and all.
    ///
    /// `pidfile.h` pads `ready` and `standby` to a fixed width, so the raw line carries
    /// trailing spaces; they are trimmed before matching. An unrecognised status is refused
    /// rather than guessed at, because whether a cluster is a standby is the fact this read
    /// exists to answer.
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        match value.trim_end() {
            "starting" => Ok(Self::Starting),
            "stopping" => Ok(Self::Stopping),
            "ready" => Ok(Self::Ready),
            "standby" => Ok(Self::Standby),
            other => Err(CollectionError::new(format!(
                "postmaster.pid reported the status {other:?}, which is not one a postmaster writes"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Stopping => "stopping",
            Self::Ready => "ready",
            Self::Standby => "standby",
        }
    }
}
