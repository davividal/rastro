//! What the healthcheck currently says about the container.

use rastro_collector::{NonEmptyText, Observation};

/// The check's own verdict, which is an observation rather than configuration.
///
/// **Volatile whole, where it is rendered**: a check that flaps moves both of these on its
/// own, and byte-identity is what every other facet rests on. The configured check beside it
/// is stable, and keeping the two apart is what lets a reader tell "somebody changed the
/// check" from "the check is currently failing".
///
/// **The log is not here, and that is the one field this facet drops rather than
/// annotates.** docker keeps the last few runs of the check with their output, and the output
/// of a failing database check is its connection error, credentials and all. It is also a
/// rolling window that changes on every run. There is no reading of it that belongs in a
/// fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedHealth {
    /// docker's own word: `starting`, `healthy`, `unhealthy`, `none`.
    pub status: NonEmptyText,
    /// How many consecutive failures the check has recorded, which is what the retry count
    /// is compared against.
    pub failing_streak: i64,
}

impl From<&ObservedHealth> for Observation {
    fn from(health: &ObservedHealth) -> Self {
        Observation::object([
            (
                "failing_streak",
                Observation::integer(health.failing_streak),
            ),
            ("status", Observation::text(health.status.as_str())),
        ])
        .volatile()
    }
}
