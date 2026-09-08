//! What the engine does when a container stops.

use rastro_collector::{NonEmptyText, Observation};

/// The policy and, where it names one, its retry limit.
///
/// **Whether a crashed container comes back is state worth diffing**, and it is state a
/// process table cannot show: a container under `unless-stopped` that is currently up looks
/// exactly like one under `no` that happens not to have crashed yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartPolicy {
    /// docker's own word: `no`, `always`, `unless-stopped`, `on-failure`.
    pub name: NonEmptyText,
    /// Absent unless the policy names a limit.
    ///
    /// docker writes `MaximumRetryCount: 0` for every policy that does not use one, and for
    /// an `on-failure` with no count given, where it means "as often as it takes". Recording
    /// the zero would read as "never retry", which is the opposite of what both mean.
    pub maximum_retries: Option<i64>,
}

impl From<&RestartPolicy> for Observation {
    fn from(policy: &RestartPolicy) -> Self {
        Observation::object([
            (
                "maximum_retries",
                match policy.maximum_retries {
                    Some(retries) => Observation::integer(retries),
                    None => Observation::null(),
                },
            ),
            ("name", Observation::text(policy.name.as_str())),
        ])
    }
}
