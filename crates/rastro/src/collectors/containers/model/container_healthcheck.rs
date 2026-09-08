//! The check the engine runs to decide whether a container is well.

use rastro_collector::Observation;

/// The check as configured, which is stable state and not an observation.
///
/// **Nanoseconds, because they are docker's own unit and the document admits no floating
/// point.** `--health-interval 30s` arrives as `30000000000`, so the timing is carried
/// exactly rather than rounded into seconds or approximated as a fraction.
///
/// The test is docker's own vector, `["CMD-SHELL", "true"]` or `["NONE"]`, kept whole: the
/// first element says how the rest is run, and flattening it into a string would lose that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHealthcheck {
    /// Plain text rather than a value object: a check's argument is an arbitrary shell
    /// command, and an empty argument is a legal one.
    pub test: Vec<String>,
    pub interval_nanoseconds: Option<i64>,
    pub timeout_nanoseconds: Option<i64>,
    /// The grace period before a failure counts, which is what stops a slow-starting
    /// service being reported unhealthy while it is still coming up.
    pub start_period_nanoseconds: Option<i64>,
    pub retries: Option<i64>,
}

impl From<&ContainerHealthcheck> for Observation {
    fn from(healthcheck: &ContainerHealthcheck) -> Self {
        Observation::object([
            (
                "interval_nanoseconds",
                number(healthcheck.interval_nanoseconds),
            ),
            ("retries", number(healthcheck.retries)),
            (
                "start_period_nanoseconds",
                number(healthcheck.start_period_nanoseconds),
            ),
            (
                "test",
                Observation::list(healthcheck.test.iter().map(Observation::text)),
            ),
            (
                "timeout_nanoseconds",
                number(healthcheck.timeout_nanoseconds),
            ),
        ])
    }
}

fn number(value: Option<i64>) -> Observation {
    match value {
        Some(value) => Observation::integer(value),
        None => Observation::null(),
    }
}
