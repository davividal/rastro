//! Which rastro produced this document, and when.

use std::time::{SystemTime, UNIX_EPOCH};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence, View,
};

use crate::collectors::filesystem::Detail;
use crate::config::Config;

const RASTRO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The settings this run resolved to, as the `invocation` facet reports them.
///
/// Shaped here rather than on `Config`, because assembling a facet's tree is
/// the collector's job and `Config` is a plain settings type.
///
/// The view is in here because it *is* part of the effective config: it is a
/// flag, and it is the single most diff-corrupting one rastro has. Diffing a
/// complete document against a diffable one would otherwise produce pages of
/// spurious removals with nothing to explain them.
///
/// The config file's path is recorded, unannotated. It is provenance, not a
/// secret: hashing it as `sensitive` would destroy the only thing it is for,
/// and omitting it for privacy would be incoherent in a tool that fingerprints
/// `/home` and the user accounts anyway.
///
/// `staged_binary` is in here because it changes what the walk reports about one
/// path, and an omission the document does not admit to is the one thing this
/// format does not do. Recorded even when false, so the key is part of the shape
/// rather than a hint that appears only on remote runs.
pub fn effective_config(
    config: &Config,
    view: View,
    staged_binary: bool,
    detail: Detail,
) -> Observation {
    Observation::object([
        ("detail", Observation::text(detail.as_str())),
        (
            "excluded_collectors",
            Observation::list(
                config
                    .excluded()
                    .iter()
                    .map(|name| Observation::text(name.as_str())),
            ),
        ),
        (
            "source",
            match config.source() {
                Some(path) => Observation::text(path),
                None => Observation::null(),
            },
        ),
        ("staged_binary", Observation::boolean(staged_binary)),
        (
            "view",
            Observation::text(match view {
                View::Diffable => "diffable",
                View::Complete => "complete",
            }),
        ),
    ])
}

pub struct InvocationCollector {
    name: FacetName,
    identity: CollectorIdentity,
    effective_config: Observation,
    walk_policy: Observation,
    observer: Option<String>,
    output: Option<String>,
    started_at: Result<i64, CollectionError>,
}

impl InvocationCollector {
    /// Takes the effective config, the effective walk table and the binary this run is
    /// reading from, rather than reading any of them, so the document's self-description is
    /// whatever the composition root actually resolved.
    ///
    /// The walk table belongs here for the same reason the config does: it is a decision
    /// this run made, not state observed on the host, and a reader looking at a tree with no
    /// digests needs one place that says which rule applied and which facet asked for it.
    /// `null` where the table could not be resolved, which is the conflict the `filesystem`
    /// facet reports as its error.
    ///
    /// The observer is here because the walk leaves it out, and an omission nothing accounts
    /// for is the one thing this format does not do.
    pub fn new(
        effective_config: Observation,
        walk_policy: Observation,
        observer: Option<String>,
        started_at: Result<i64, CollectionError>,
        output: Option<String>,
    ) -> Self {
        Self {
            effective_config,
            walk_policy,
            observer,
            output,
            started_at,
            name: FacetName::new("invocation").expect("`invocation` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("invocation").expect("`invocation` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
        }
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
        let started_at = self.started_at.as_ref().map_err(|error| error.clone())?;

        Ok(Observation::object([
            ("rastro_version", Observation::text(RASTRO_VERSION)),
            ("config", self.effective_config.clone()),
            (
                "observer",
                match &self.observer {
                    Some(binary) => Observation::text(binary.as_str()).volatile(),
                    None => Observation::null(),
                },
            ),
            (
                "output",
                match &self.output {
                    Some(path) => Observation::text(path.as_str()).volatile(),
                    None => Observation::null(),
                },
            ),
            ("started_at", Observation::integer(*started_at).volatile()),
            ("walk_policy", self.walk_policy.clone()),
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
