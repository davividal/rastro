//! The program a unit starts.

use rastro_collector::{CollectionError, NonEmptyText};

/// The binary systemd will execute, exactly as the unit spells it.
///
/// **Not an [`AbsolutePath`](rastro_collector::AbsolutePath), and that is measured rather
/// than lax.** systemd accepts a bare program name and resolves it against its own
/// compiled-in search list, and Debian 12 ships units that use it:
/// `systemd-tmpfiles-setup-dev.service` has `path=systemd-tmpfiles`. Insisting on a leading
/// `/` would turn a unit that works into a collection failure, and the path rastro records
/// is the one the host declared rather than one rastro resolved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExecutablePath(NonEmptyText);

impl ExecutablePath {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "unit executable")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
