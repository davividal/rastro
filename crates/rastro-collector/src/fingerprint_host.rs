//! The use case: fingerprint this host.

use crate::{Collector, Presence};
use rastro_fingerprint::FingerprintError;
use rastro_fingerprint::{Facet, FacetName, FacetOutcome, Fingerprint};

/// What a caller wants to be told while a run is under way.
///
/// **No clock here, deliberately.** This crate must not read the host, so *when*
/// something happened is the tool's observation rather than this use case's: the
/// callbacks say what, the caller times it. That is not merely a rule obeyed, it
/// is why the seam exists in this shape — a run's timings must never be able to
/// reach the document, and a library that cannot see a clock cannot put one there.
///
/// Every method does nothing by default, so a caller implements only the ones it
/// wants. `&self`, so state lives behind a `Cell` in the caller and no collector
/// has to become mutable to be reported on.
pub trait RunProgress {
    fn collector_started(&self, name: &FacetName) {
        let _ = name;
    }

    fn collector_finished(&self, name: &FacetName, outcome: &FacetOutcome) {
        let _ = (name, outcome);
    }
}

/// The unreported run, which is what [`run`] has always been.
struct Unreported;

impl RunProgress for Unreported {}

/// Asks every collector for its report and assembles them into a fingerprint.
///
/// Fails only if the collectors themselves are inconsistent, for instance two
/// claiming the same facet name. A collector that is absent or that fails does
/// not fail the run: that is what `absent` and `error` facets are for.
pub fn run(collectors: &[Box<dyn Collector>]) -> Result<Fingerprint, FingerprintError> {
    run_reporting(collectors, &Unreported)
}

/// The same run, telling `progress` about each collector as it goes.
///
/// Collectors are asked in registration order and the document is sorted by name
/// afterwards, so what a caller is told is deterministic and independent of what
/// ends up where in the document.
pub fn run_reporting(
    collectors: &[Box<dyn Collector>],
    progress: &dyn RunProgress,
) -> Result<Fingerprint, FingerprintError> {
    let facets = collectors.iter().map(|collector| {
        let collector = collector.as_ref();
        progress.collector_started(collector.name());

        let facet = facet_of(collector);
        progress.collector_finished(&facet.name, &facet.outcome);

        facet
    });

    Fingerprint::from_facets(facets)
}

/// Turns one collector's answers into a facet.
///
/// The only place that decides what absence and failure mean for a document, so
/// no adapter has to know.
fn facet_of(collector: &dyn Collector) -> Facet {
    let outcome = match collector.presence() {
        Presence::Absent => FacetOutcome::Absent,
        Presence::Undetermined { reason } => FacetOutcome::error(reason),
        Presence::Present => match collector.collect() {
            Ok(observation) => FacetOutcome::ok(observation),
            Err(failure) => FacetOutcome::error(failure.to_string()),
        },
    };

    Facet::new(
        collector.name().clone(),
        collector.identity().clone(),
        collector.category(),
        outcome,
    )
}
