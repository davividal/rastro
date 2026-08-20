//! Whether a unit file is enabled.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// Whether a unit file is enabled, and how.
///
/// `enabled`, `enabled-runtime`, `disabled`, `static`, `alias`, `masked`,
/// `masked-runtime`, `linked`, `linked-runtime`, `indirect`, `generated`, `transient`
/// and `bad`. On the development box nine of the thirteen appear at once.
///
/// **This is the single most valuable field in the facet.** `design.md` names an
/// enablement symlink under a `*.wants/` directory as exactly what this tool exists to
/// catch, and this is where such a change surfaces: enabling a service moves it from
/// `disabled` to `enabled` and touches no other state on the box.
///
/// **Text rather than an enum, and unlike `ArchiveType` that is the right call here.**
/// apt has accepted exactly two archive types forever, so a third word proves a
/// misread. systemd's table is not closed in the same way: it has grown over the
/// project's life, and refusing an unrecognised state would make rastro fail on a box
/// running a newer systemd than rastro was built against. That is the same reasoning
/// the password hash algorithm uses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitFileState(NonEmptyText);

impl UnitFileState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "unit file state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&UnitFileState> for Observation {
    fn from(value: &UnitFileState) -> Self {
        Observation::text(value.as_str())
    }
}
