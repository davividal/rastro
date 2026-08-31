//! The use case: fingerprint this host.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Collector, Concurrency, Presence};
use rastro_fingerprint::FingerprintError;
use rastro_fingerprint::{Facet, FacetName, FacetOutcome, Fingerprint};

/// How many collectors run at once.
///
/// Four rather than one per core. Almost every collector spawns a subprocess, and a fingerprint
/// that starts twenty tools at once on a production box is an intrusion of its own — the thing
/// this tool exists not to be. What the wait is made of is latency rather than CPU, so a small
/// pool recovers nearly all of it: 0.69 s of serial subprocess time on a reference box becomes
/// roughly a quarter of that.
const AT_ONCE: usize = 4;

/// The collectors of one kind, in the order they were registered.
type Registered<'a> = Vec<&'a dyn Collector>;

/// What a caller wants to be told while a run is under way.
///
/// **No clock here, deliberately.** This crate must not read the host, so *when*
/// something happened is the tool's observation rather than this use case's: the
/// callbacks say what, the caller times it. That is not merely a rule obeyed, it
/// is why the seam exists in this shape — a run's timings must never be able to
/// reach the document, and a library that cannot see a clock cannot put one there.
///
/// Every method does nothing by default, so a caller implements only the ones it
/// wants. `&self`, so no collector has to become mutable to be reported on — and
/// `Send + Sync`, because collectors run concurrently and these fire from whichever
/// worker got there, which is also why a caller's counters have to be atomic.
///
/// **Told in completion order, not registration order.** The point of hearing about a
/// collector starting is to say so while it is still running, so the events cannot be held
/// back and reordered. A caller that wants a stable report sorts what it collected.
pub trait RunProgress: Send + Sync {
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
/// **Shared collectors run together on a small pool; exclusive ones run alone afterwards.**
/// Most of a run is spent waiting for a subprocess to answer, so waiting in parallel is where
/// the remaining time is. The filesystem walk is the one collector that cannot tolerate
/// company — see [`Concurrency`] — and it runs last, by itself, with nothing else able to
/// disturb the tree it is describing.
///
/// The order facets come back in does not matter: [`Fingerprint::from_facets`] sorts by name,
/// so the document is identical however the workers were scheduled.
pub fn run_reporting(
    collectors: &[Box<dyn Collector>],
    progress: &dyn RunProgress,
) -> Result<Fingerprint, FingerprintError> {
    let (shared, alone): (Registered, Registered) = collectors
        .iter()
        .map(Box::as_ref)
        .partition(|collector| collector.concurrency() == Concurrency::Shared);

    let collected = Mutex::new(Vec::with_capacity(collectors.len()));
    let next = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..AT_ONCE.min(shared.len()) {
            scope.spawn(|| {
                // A shared cursor rather than a slice each, because the collectors differ by
                // orders of magnitude in cost and a fixed split would leave workers idle
                // while one of them finished `systemctl`.
                while let Some(collector) = shared.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let facet = reported(*collector, progress);
                    collected
                        .lock()
                        .expect("a collector that panicked would have unwound the scope")
                        .push(facet);
                }
            });
        }
    });

    let mut facets = collected
        .into_inner()
        .expect("the scope has joined every worker");

    for collector in alone {
        facets.push(reported(collector, progress));
    }

    Fingerprint::from_facets(facets)
}

/// One collector's facet, with the caller told before and after.
fn reported(collector: &dyn Collector, progress: &dyn RunProgress) -> Facet {
    progress.collector_started(collector.name());

    let facet = facet_of(collector);
    progress.collector_finished(&facet.name, &facet.outcome);

    facet
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
