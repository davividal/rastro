//! The files the host's timekeeping is configured in.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use crate::collectors::time::model::ClockSettings;
use crate::collectors::time::value_objects::Timezone;

/// Debian's plain-text timezone name.
const ETC_TIMEZONE: &str = "/etc/timezone";

/// The symlink into the zoneinfo database that every program actually follows.
const ETC_LOCALTIME: &str = "/etc/localtime";

/// Where `hwclock` records which scale the hardware clock runs on.
const ETC_ADJTIME: &str = "/etc/adjtime";

/// The stamp `systemd-timesyncd` drops once it has synchronised.
const TIMESYNC_STAMP: &str = "/run/systemd/timesync/synchronized";

/// The directory the `/etc/localtime` symlink points into.
///
/// The zone name is what follows it, so `/usr/share/zoneinfo/Etc/UTC` gives `Etc/UTC`.
const ZONEINFO: &str = "/usr/share/zoneinfo/";

/// What `/etc/adjtime`'s third line says when the hardware clock runs on local time.
const LOCAL: &str = "LOCAL";

/// The host's timekeeping, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockFiles {
    timezone: PathBuf,
    localtime: PathBuf,
    adjtime: PathBuf,
    synchronised: PathBuf,
}

impl ClockFiles {
    pub fn new() -> Self {
        Self {
            timezone: PathBuf::from(ETC_TIMEZONE),
            localtime: PathBuf::from(ETC_LOCALTIME),
            adjtime: PathBuf::from(ETC_ADJTIME),
            synchronised: PathBuf::from(TIMESYNC_STAMP),
        }
    }

    /// The same rooted under a directory the caller chose, which is what makes this
    /// testable without an `/etc`.
    pub fn under(root: &Path) -> Self {
        let joined = |absolute: &str| root.join(absolute.trim_start_matches('/'));

        Self {
            timezone: joined(ETC_TIMEZONE),
            localtime: joined(ETC_LOCALTIME),
            adjtime: joined(ETC_ADJTIME),
            synchronised: joined(TIMESYNC_STAMP),
        }
    }

    pub fn read(&self) -> Result<ClockSettings, CollectionError> {
        Ok(ClockSettings {
            timezone: self.read_timezone()?,
            local_real_time_clock: self.read_local_clock()?,
            // Existence is the fact. The file's contents are empty and its mtime moves with
            // every synchronisation, so only whether it is there is state.
            synchronised: self.synchronised.exists(),
        })
    }

    /// The configured zone, from whichever of the two places names it.
    ///
    /// **`/etc/localtime` is preferred and `/etc/timezone` is the fallback, because they can
    /// disagree and only the first one is obeyed.** Every program resolving a local time
    /// follows the symlink; `/etc/timezone` is a Debian convenience that `dpkg-reconfigure`
    /// keeps in step and a hand edit does not. Reading the file first would report the zone
    /// the box is documented to be in rather than the one it is in.
    fn read_timezone(&self) -> Result<Option<Timezone>, CollectionError> {
        if let Ok(target) = fs::read_link(&self.localtime) {
            let target = target.to_string_lossy().into_owned();
            if let Some(zone) = target.split_once(ZONEINFO) {
                return Ok(Some(Timezone::new(zone.1)?));
            }
        }

        let Some(named) = read_optional(&self.timezone)? else {
            return Ok(None);
        };
        let named = named.trim();
        if named.is_empty() {
            return Ok(None);
        }

        Ok(Some(Timezone::new(named)?))
    }

    /// Whether the hardware clock runs on local time rather than UTC.
    ///
    /// **An absent `/etc/adjtime` means UTC, and that is the file's documented contract
    /// rather than a guess.** `hwclock` writes the file only once it has something to record,
    /// so a box that has never been told otherwise — including the development box, which has
    /// no such file — is running its hardware clock on UTC. Treating the absence as unknown
    /// would make the common case a null.
    fn read_local_clock(&self) -> Result<bool, CollectionError> {
        let Some(contents) = read_optional(&self.adjtime)? else {
            return Ok(false);
        };

        // The third line is the scale. The first two are the drift factor and the last
        // adjustment time, both of which move on their own and neither of which is read.
        Ok(contents
            .lines()
            .nth(2)
            .is_some_and(|scale| scale.trim() == LOCAL))
    }
}

fn read_optional(path: &Path) -> Result<Option<String>, CollectionError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CollectionError::new(format!(
            "could not read {}: {error}",
            path.display()
        ))),
    }
}

impl Default for ClockFiles {
    fn default() -> Self {
        Self::new()
    }
}
