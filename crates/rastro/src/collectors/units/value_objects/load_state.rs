//! Whether systemd could load a unit at all.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// Whether systemd could load the unit at all.
///
/// `loaded`, `not-found`, `bad-setting`, `error`, `masked`, `stub` or `merged`.
///
/// **`not-found` is the interesting one and it really occurs.** Seventy-one units on
/// the development box are loaded with no unit file behind them, and among them are
/// `NetworkManager.service` and `auditd.service`: something in the dependency graph
/// references them and they are not installed. A fingerprint that only listed unit
/// files would say nothing about a dangling reference like that.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LoadState(NonEmptyText);

impl LoadState {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "load state")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&LoadState> for Observation {
    fn from(value: &LoadState) -> Self {
        Observation::text(value.as_str())
    }
}
