//! What a run tells the operator while it is happening, and what it cost.
//!
//! **Never the document.** A fingerprint records what a box *is*, not what it is doing, so a
//! duration belongs on stderr and nowhere else. The seam in `rastro-collector` is what makes
//! that structural rather than a promise: the library is handed no clock, so it could not
//! write one into a facet if it wanted to.
//!
//! Two sinks are needed and one type provides both, because they see different things.
//! `RunProgress` is the port every collector passes through, so it can time each facet but
//! cannot see inside one. The filesystem walk reports its own progress through
//! [`WalkProgress`], because from the outside it is one collector that takes a while.
//!
//! **Everything here is shared across threads**, since collectors run concurrently: the
//! counters are atomic, and the one line the live counter draws on is behind a mutex so two
//! workers cannot interleave escape sequences on it.

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rastro_collector::fingerprint_host::RunProgress;
use rastro_fingerprint::{FacetName, FacetOutcome};

/// At most one redraw this often, because the terminal must not become the bottleneck.
const REDRAW_EVERY: Duration = Duration::from_millis(100);

/// And at most one clock reading per this many entries, because the counter is called once per
/// path and `Instant::now` is not free at that rate.
const CHECK_EVERY: u64 = 512;

/// What the filesystem walk tells whoever is watching it.
///
/// A second trait rather than more methods on `RunProgress`, because that port sees a collector
/// from the outside: it knows the walk started and finished, and nothing about the tens of
/// thousands of entries in between. This is the seam that makes a long run legible.
/// `Send + Sync` because the walk is one collector among several running at once, and this is
/// shared with whoever is watching all of them.
pub trait WalkProgress: Send + Sync {
    fn entry_walked(&self) {
        // Called once per path, so a default that does nothing has to cost nothing.
    }
}

/// A run being watched: what each collector costs, and what the walk does.
///
/// One type for both jobs because both are driven by the same events, and a run wanting a live
/// counter usually wants the summary too. Either half can be off.
pub struct Reporting {
    live: bool,
    started: Instant,
    /// When each in-flight collector started, keyed by name.
    ///
    /// A map rather than one slot, because several run at once and a single "current" would
    /// attribute one collector's time to another.
    running: Mutex<HashMap<String, Instant>>,
    finished: Mutex<Vec<Timing>>,
    entries: AtomicU64,
    /// Held across a redraw, so two workers cannot interleave escape sequences on one line.
    drawing: Mutex<Option<Instant>>,
}

/// One collector's turn, named rather than a bare triple.
#[derive(Clone)]
struct Timing {
    name: String,
    elapsed: Duration,
    status: &'static str,
}

impl Reporting {
    /// A sink that draws a live counter or not.
    ///
    /// Built before anything runs, because the total it reports is the whole run rather than
    /// the part after somebody thought to start a clock.
    pub fn new(live: bool) -> Self {
        Self {
            live,
            started: Instant::now(),
            running: Mutex::new(HashMap::new()),
            finished: Mutex::new(Vec::new()),
            entries: AtomicU64::new(0),
            drawing: Mutex::new(None),
        }
    }

    /// Clears the counter's line, so a warning or an error is never half-overwritten by it.
    pub fn clear(&self) {
        if !self.live {
            return;
        }

        let _held = self.drawing.lock();
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[K");
        let _ = stderr.flush();
    }

    /// The `--debug` report, written after the document is safely on disk.
    ///
    /// **Sorted by name, not by cost and not by the order they finished.** Collectors run
    /// concurrently, so completion order is whichever worker got there and would differ
    /// between two runs of an unchanged host — which is exactly what a report meant for
    /// comparing runs must not do. Name order is deterministic and matches the document's own.
    ///
    /// Peak resident memory from `VmHWM`, because that is the number that decides whether the
    /// walk needs to stop building the document in memory at all, and guessing at it is what
    /// made this change take two attempts.
    pub fn report(&self, out: &mut impl Write, wrote: &str) -> std::io::Result<()> {
        writeln!(out, "rastro: debug")?;

        let mut timings = self
            .finished
            .lock()
            .expect("a panicking collector would have unwound the run")
            .clone();
        timings.sort_by(|left, right| left.name.cmp(&right.name));

        for timing in &timings {
            writeln!(
                out,
                "  {:<24}{:>9.3} s  {}",
                timing.name,
                timing.elapsed.as_secs_f64(),
                timing.status
            )?;
        }

        writeln!(
            out,
            "  {:<24}{:>9.3} s",
            "total",
            self.started.elapsed().as_secs_f64()
        )?;
        writeln!(
            out,
            "  walk: {} entries",
            self.entries.load(Ordering::Relaxed)
        )?;
        writeln!(out, "  wrote {wrote}")?;

        if let Some(peak) = peak_resident_kilobytes() {
            writeln!(out, "  peak resident {}", human_bytes(peak * 1024))?;
        }

        Ok(())
    }

    /// Redraws the counter, unless it was drawn too recently or is switched off.
    ///
    /// **A counter, not a bar, and that is a decision rather than a shortcut.** The walk finds
    /// its own work as it goes, so a percentage needs a denominator that does not exist yet.
    /// The one cheap source of a bound is the used-inode count per mount, which needs `statfs`
    /// — a syscall std does not wrap, so reaching it would cost `#![deny(unsafe_code)]`, and
    /// even then it would bound entries rather than time. A number that slides smoothly and
    /// means nothing is worse than an honest count.
    fn draw(&self, force: bool) {
        if !self.live {
            return;
        }

        // One lock for the throttle and the write together, so two workers cannot interleave
        // escape sequences on the one line they share.
        //
        // **A poisoned lock is taken anyway rather than branched on.** Poisoning means some other
        // thread panicked while drawing, and what it guards is one timestamp behind a cosmetic
        // counter: there is no invariant left half-built for this to inherit. Skipping the redraw
        // instead would have been an unreachable branch guarding nothing, and panicking would
        // kill a fingerprint run over a spinner frame.
        let mut last_drawn = self
            .drawing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let now = Instant::now();
        let too_soon = last_drawn.is_some_and(|last| now.duration_since(last) < REDRAW_EVERY);
        if too_soon && !force {
            return;
        }
        *last_drawn = Some(now);

        let in_flight = self
            .running
            .lock()
            .map(|running| running.len())
            .unwrap_or(0);
        let elapsed = self.started.elapsed().as_secs();
        let mut stderr = std::io::stderr().lock();
        let _ = write!(
            stderr,
            "\r\x1b[K{in_flight} collecting  {} entries  {:02}:{:02}",
            self.entries.load(Ordering::Relaxed),
            elapsed / 60,
            elapsed % 60
        );
        let _ = stderr.flush();
    }
}

impl RunProgress for Reporting {
    fn collector_started(&self, name: &FacetName) {
        if let Ok(mut running) = self.running.lock() {
            running.insert(name.as_str().to_owned(), Instant::now());
        }
        self.draw(true);
    }

    fn collector_finished(&self, name: &FacetName, outcome: &FacetOutcome) {
        let elapsed = self
            .running
            .lock()
            .ok()
            .and_then(|mut running| running.remove(name.as_str()))
            .map(|at| at.elapsed())
            .unwrap_or_default();

        let status = match outcome {
            FacetOutcome::Ok { .. } => "ok",
            FacetOutcome::Absent => "absent",
            FacetOutcome::Error { .. } => "error",
        };

        if let Ok(mut finished) = self.finished.lock() {
            finished.push(Timing {
                name: name.as_str().to_owned(),
                elapsed,
                status,
            });
        }
        self.draw(false);
    }
}

impl WalkProgress for Reporting {
    fn entry_walked(&self) {
        let seen = self.entries.fetch_add(1, Ordering::Relaxed) + 1;

        if seen.is_multiple_of(CHECK_EVERY) {
            self.draw(false);
        }
    }
}

/// Whether a live counter would be going to a terminal rather than into a pipe.
///
/// The gate that keeps "a clean redirected run says nothing on stderr" true by construction
/// rather than by anybody remembering it: a redirected stderr is somebody capturing output, and
/// a spinner in a log file is noise nobody asked for.
pub fn stderr_is_a_terminal() -> bool {
    std::io::stderr().is_terminal()
}

/// Peak resident set size in kilobytes, from `/proc/self/status`.
///
/// `None` off Linux and on a kernel that does not report it, because an absent measurement is
/// better than a made-up one.
fn peak_resident_kilobytes() -> Option<u64> {
    peak_resident_in(&std::fs::read_to_string("/proc/self/status").ok()?)
}

/// The `VmHWM` line of `/proc/self/status`, in kilobytes.
///
/// Separate from the read so both answers come from a fixture: a kernel built without `VmHWM`
/// reports every other field and simply omits that line, which is not something a test can ask
/// the kernel it is running on to do.
pub fn peak_resident_in(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
}

/// Bytes at a scale a human reads, to one decimal.
///
/// Public so the scales are stated by a test rather than inferred from one `--debug` run: the
/// only caller passes `VmHWM`, which is kilobytes and so never small enough to reach the plain
/// byte case.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut scaled = bytes as f64;
    let mut unit = 0;

    while scaled >= 1024.0 && unit < UNITS.len() - 1 {
        scaled /= 1024.0;
        unit += 1;
    }

    match unit {
        0 => format!("{bytes} B"),
        _ => format!("{scaled:.1} {}", UNITS[unit]),
    }
}
