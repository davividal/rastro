//! Which rastro produced this document, and when.

use std::time::{SystemTime, UNIX_EPOCH};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

const RASTRO_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct InvocationCollector {
    name: FacetName,
    identity: CollectorIdentity,
}

impl InvocationCollector {
    pub fn new() -> Self {
        Self {
            name: FacetName::new("invocation").expect("`invocation` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("invocation").expect("`invocation` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
        }
    }
}

impl Default for InvocationCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for InvocationCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::Metadata
    }

    /// Always present: the run is describing itself.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let started_at = seconds_since_epoch(SystemTime::now())?;

        Ok(Observation::object([
            ("rastro_version", Observation::text(RASTRO_VERSION)),
            ("started_at", Observation::integer(started_at).volatile()),
        ]))
    }
}

/// A clock reading as the format's integer type.
///
/// Takes the reading rather than calling the clock, so the failure a host can
/// actually produce is reachable from a test: a box whose clock is set before
/// 1970 gets a recorded `error` facet, not a wrapped-around timestamp.
///
/// The second arm cannot fire on any supported platform, since `SystemTime` is
/// backed by a `timespec` whose seconds field is already an `i64`. It is
/// written out rather than cast away because `as i64` would turn a clock beyond
/// the range into a negative timestamp silently, and a fingerprint that lies
/// quietly is the one failure mode this project will not accept.
pub fn seconds_since_epoch(clock_reading: SystemTime) -> Result<i64, CollectionError> {
    let since_epoch = clock_reading.duration_since(UNIX_EPOCH).map_err(|_| {
        CollectionError::new(
            "the system clock is set before 1970, so this run cannot be timestamped",
        )
    })?;

    i64::try_from(since_epoch.as_secs()).map_err(|_| {
        CollectionError::new("the system clock is too far ahead to record as a second count")
    })
}
