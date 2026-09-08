//! One `ulimit` a container runs under.

use rastro_collector::Observation;

/// The soft and hard halves of a limit, which are different facts.
///
/// The soft limit is what a process starts with and may raise up to the hard one; the hard
/// one it cannot raise at all. `--ulimit nofile=1024:4096` sets them apart, and a
/// single-figure ulimit sets both to the same value, which is docker's own doing rather
/// than rastro's reading of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimit {
    pub soft: i64,
    pub hard: i64,
}

impl From<&ResourceLimit> for Observation {
    fn from(limit: &ResourceLimit) -> Self {
        Observation::object([
            ("hard", Observation::integer(limit.hard)),
            ("soft", Observation::integer(limit.soft)),
        ])
    }
}
