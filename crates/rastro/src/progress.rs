//! What a run tells the operator while it is happening, and what it cost.
//!
//! **Never the document.** A fingerprint records what a box *is*, not what it is doing, so a
//! duration belongs on stderr and nowhere else. The seam in `rastro-collector` is what makes
//! that structural rather than a promise: the library is handed no clock, so it could not
//! write one into a facet if it wanted to.
//!
//! Two sinks, because they see different things. `RunProgress` is the port every collector
//! passes through, so it can time each facet but cannot see inside one. The filesystem walk is
//! the collector that needed watching, and it reports its own counters through
//! [`WalkProgress`].

use std::cell::{Cell, RefCell};
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use rastro_collector::fingerprint_host::RunProgress;
use rastro_fingerprint::{FacetName, FacetOutcome};

/// What the filesystem walk tells whoever is watching it.
///
/// A second sink rather than more methods on `RunProgress`, because that port sees a collector
/// from the outside: it knows the walk started and finished, and nothing about the 46,000
/// entries in between. This is the seam that made a 51-minute run legible.
pub trait WalkProgress {
    fn entry_walked(&self) {
        // Called once per path, so the default has to be free rather than merely cheap.
    }

    fn file_opened(&self) {}

    fn bytes_hashed(&self, bytes: u64) {
        let _ = bytes;
    }
}

/// A run being watched: what each collector cost, and what the walk did.
pub struct Reporting {
    started: Instant,
    current: RefCell<Option<(String, Instant)>>,
    collectors: RefCell<Vec<(String, Duration, &'static str)>>,
    entries: Cell<u64>,
    files_opened: Cell<u64>,
    bytes_hashed: Cell<u64>,
}

impl Reporting {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            current: RefCell::new(None),
            collectors: RefCell::new(Vec::new()),
            entries: Cell::new(0),
            files_opened: Cell::new(0),
            bytes_hashed: Cell::new(0),
        }
    }

    /// The `--debug` report, written after the document is safely on disk.
    ///
    /// In registration order rather than slowest-first, so two runs are comparable line by
    /// line. Peak resident memory from `VmHWM`, because that is the number that decides
    /// whether the walk needs to stop building the document in memory at all, and guessing at
    /// it is what made this whole change take two attempts.
    pub fn report(&self, out: &mut impl Write, wrote: &str) -> std::io::Result<()> {
        writeln!(out, "rastro: debug")?;

        for (name, elapsed, status) in self.collectors.borrow().iter() {
            writeln!(
                out,
                "  {name:<24}{:>9.3} s  {status}",
                elapsed.as_secs_f64()
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

    pub fn entries(&self) -> u64 {
        self.entries.get()
    }
}

impl Default for Reporting {
    fn default() -> Self {
        Self::new()
    }
}

impl RunProgress for Reporting {
    fn collector_started(&self, name: &FacetName) {
        *self.current.borrow_mut() = Some((name.as_str().to_owned(), Instant::now()));
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

        self.collectors
            .borrow_mut()
            .push((name.as_str().to_owned(), elapsed, status));
    }
}

impl WalkProgress for Reporting {
    fn entry_walked(&self) {
        self.entries.set(self.entries.get() + 1);
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
/// The gate that keeps "a clean run says nothing on stderr" true by construction rather than
/// by remembering: a redirected stderr is somebody capturing output, and a spinner in a log
/// file is noise nobody asked for.
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
