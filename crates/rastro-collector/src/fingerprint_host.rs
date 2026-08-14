//! The use case: fingerprint this host.

use crate::{Collector, Presence};
use rastro_fingerprint::FingerprintError;
use rastro_fingerprint::{Facet, FacetOutcome, Fingerprint};

/// Asks every collector for its report and assembles them into a fingerprint.
///
/// Fails only if the collectors themselves are inconsistent, for instance two
/// claiming the same facet name. A collector that is absent or that fails does
/// not fail the run: that is what `absent` and `error` facets are for.
pub fn run(collectors: &[Box<dyn Collector>]) -> Result<Fingerprint, FingerprintError> {
    let facets = collectors
        .iter()
        .map(|collector| facet_of(collector.as_ref()));
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
