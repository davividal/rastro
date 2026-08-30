//! What a run tells the operator while it is happening, and what it cost.
//!
//! **Never the document.** A fingerprint records what a box *is*, not what it is doing, so a
//! duration belongs on stderr and nowhere else. The seam in `rastro-collector` is what makes
//! that structural rather than a promise: the library is handed no clock, so it could not
//! write one into a facet if it wanted to.
//!
//! Two sinks are needed and one type provides both, because they see different things.
//! `RunProgress` is the port every collector passes through, so it can time each facet but
//! cannot see inside one. The filesystem walk is the collector that needed watching — it was
//! 99.8% of a measured run — and it reports its own counters through [`WalkProgress`].

use std::cell::{Cell, RefCell};
use std::io::{IsTerminal, Write};
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
pub trait WalkProgress {
    fn entry_walked(&self) {
        // Called once per path, so a default that does nothing has to cost nothing.
    }

    fn file_opened(&self) {}

    fn bytes_hashed(&self, bytes: u64) {
        let _ = bytes;
    }
}

/// A run being watched: what each collector costs, and what the walk does.
///
/// One type for both jobs because both are driven by the same events, and a run wanting a live
/// counter usually wants the summary too. Either half can be off.
pub struct Reporting {
    live: bool,
    started: Instant,
    current: RefCell<Option<(String, Instant)>>,
    collectors: RefCell<Vec<Timing>>,
    entries: Cell<u64>,
    files_opened: Cell<u64>,
    bytes_hashed: Cell<u64>,
    last_drawn: Cell<Option<Instant>>,
}

/// One collector's turn, named rather than a bare triple.
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
            current: RefCell::new(None),
            collectors: RefCell::new(Vec::new()),
            entries: Cell::new(0),
            files_opened: Cell::new(0),
            bytes_hashed: Cell::new(0),
            last_drawn: Cell::new(None),
        }
    }

    /// Clears the counter's line, so a warning or an error is never half-overwritten by it.
    pub fn clear(&self) {
        if !self.live {
            return;
        }

        let mut stderr = std::io::stderr();
        let _ = write!(stderr, "\r\x1b[K");
        let _ = stderr.flush();
    }

    /// The `--debug` report, written after the document is safely on disk.
    ///
    /// In registration order rather than slowest-first, so two runs are comparable line by
    /// line. Peak resident memory from `VmHWM`, because that is the number that decides
    /// whether the walk needs to stop building the document in memory at all, and guessing at
    /// it is what made this change take two attempts.
    pub fn report(&self, out: &mut impl Write, wrote: &str) -> std::io::Result<()> {
        writeln!(out, "rastro: debug")?;

        for timing in self.collectors.borrow().iter() {
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
            "  walk: {} entries, {} files opened, {} hashed",
            self.entries.get(),
            self.files_opened.get(),
            human_bytes(self.bytes_hashed.get())
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

        let now = Instant::now();
        let too_soon = self
            .last_drawn
            .get()
            .is_some_and(|last| now.duration_since(last) < REDRAW_EVERY);
        if too_soon && !force {
            return;
        }
        self.last_drawn.set(Some(now));

        let running = self
            .current
            .borrow()
            .as_ref()
            .map(|(name, _)| name.clone())
            .unwrap_or_default();

        let elapsed = self.started.elapsed().as_secs();
        let mut stderr = std::io::stderr();
        let _ = write!(
            stderr,
            "\r\x1b[K{running:<16} {} entries  {:02}:{:02}",
            self.entries.get(),
            elapsed / 60,
            elapsed % 60
        );
        let _ = stderr.flush();
    }
}

impl RunProgress for Reporting {
    fn collector_started(&self, name: &FacetName) {
        *self.current.borrow_mut() = Some((name.as_str().to_owned(), Instant::now()));
        self.draw(true);
    }

    fn collector_finished(&self, name: &FacetName, outcome: &FacetOutcome) {
        let elapsed = self
            .current
            .borrow_mut()
            .take()
            .filter(|(started, _)| started == name.as_str())
            .map(|(_, at)| at.elapsed())
            .unwrap_or_default();

        let status = match outcome {
            FacetOutcome::Ok { .. } => "ok",
            FacetOutcome::Absent => "absent",
            FacetOutcome::Error { .. } => "error",
        };

        self.collectors.borrow_mut().push(Timing {
            name: name.as_str().to_owned(),
            elapsed,
            status,
        });
    }
}

impl WalkProgress for Reporting {
    fn entry_walked(&self) {
        let seen = self.entries.get() + 1;
        self.entries.set(seen);

        if seen.is_multiple_of(CHECK_EVERY) {
            self.draw(false);
        }
    }

    fn file_opened(&self) {
        self.files_opened.set(self.files_opened.get() + 1);
    }

    fn bytes_hashed(&self, bytes: u64) {
        self.bytes_hashed.set(self.bytes_hashed.get() + bytes);
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
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
}

/// Bytes at a scale a human reads, to one decimal.
fn human_bytes(bytes: u64) -> String {
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
