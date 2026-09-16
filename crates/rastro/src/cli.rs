//! The command line as the operator meets it.

use std::path::{Path, PathBuf};

use clap::Parser;

use rastro_fingerprint::{Presentation, View};

/// Emits a canonical, diffable fingerprint of the current host's state.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Also emit values that change on their own between runs, such as PIDs and
    /// the run timestamp.
    ///
    /// Two runs of an unchanged host are then no longer byte-identical, which
    /// is exactly why this is not the default: a fingerprint you cannot diff
    /// cleanly is the problem rastro exists to solve.
    #[arg(long)]
    include_volatile: bool,

    /// Narrow this run with a config file.
    ///
    /// Optional and explicit: with no `--config` every collector runs, because
    /// the premise is a box nobody documented. There is no auto-discovery, so a
    /// stale file lying beside the binary cannot quietly narrow a run.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// This binary is a temporary copy, so do not report it as part of the host.
    ///
    /// For a caller that staged the executable and will delete it, which is what
    /// `rastro-ssh` does with `mktemp /var/tmp/rastro.XXXXXXXX`. Without this the
    /// walk reports the file it is running from like any other, because a rastro
    /// installed on a box *is* part of that box and a swapped binary is exactly
    /// the change a fingerprint should catch.
    ///
    /// rastro cannot tell the two apart by itself: the staged copy and an
    /// installed one are byte-identical and the kernel vouches for both, so the
    /// only party that knows is the one that made the copy.
    #[arg(long)]
    staged: bool,

    /// Where to write the fingerprint. `-` means stdout.
    ///
    /// Without this the document goes to `./rastro-<host>-<UTC>.json`, because a
    /// fingerprint of a real host is megabytes and a default that puts megabytes
    /// on a terminal punishes the first run. The file is created `0600`: it names
    /// every path on the box.
    #[arg(short = 'o', long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Overwrite the output file if it is already there.
    ///
    /// Off by default because the workflow is a `before` and an `after`, and
    /// replacing the `before` destroys the only record of the state being
    /// compared against. That is the one irreversible thing rastro can do.
    #[arg(long)]
    force: bool,

    /// Show a live counter on stderr while the run is happening.
    ///
    /// On by default when stderr is a terminal and off when it is redirected,
    /// because a spinner in a log file is noise nobody asked for. These force it
    /// either way.
    ///
    /// A counter and not a bar: the walk discovers its own work as it goes, so a
    /// percentage would need a denominator nobody has.
    #[arg(long, conflicts_with = "no_progress")]
    progress: bool,

    /// Never show the live counter, whatever stderr is attached to.
    #[arg(long)]
    no_progress: bool,

    /// Report what the run cost, on stderr: per collector, plus what the walk read.
    ///
    /// `time ./rastro > file` answers neither "which collector was slow" nor
    /// "where did the document go", and those are the two questions a slow run
    /// actually raises. Never written into the document: a fingerprint records
    /// what a box is, not what it is doing.
    #[arg(long)]
    debug: bool,

    /// Record every attribute of every walked path, rather than one digest of them.
    ///
    /// The default records a digest per path, which answers what a fingerprint is
    /// taken to answer — did anything about this path change — and costs a fifth
    /// of the document. This asks *which* attribute moved, and it has to be asked
    /// at the time: a summary taken yesterday cannot be expanded today.
    #[arg(long)]
    detail: bool,

    /// Show values a collector marked sensitive as they stand, rather than as a
    /// digest.
    ///
    /// Off by default, so a caller who has never thought about disclosure still
    /// gets the safe document. What this produces is a file holding every secret
    /// the collectors read, which is worth knowing before piping it anywhere: the
    /// default `0600` is the only thing between it and the next reader.
    ///
    /// A redacted value still diffs. The digest moves when the value behind it
    /// moves, so `--raw` buys the ability to *read* a secret, never the ability
    /// to detect that one changed.
    #[arg(long)]
    raw: bool,
}

impl Cli {
    /// The config file the operator named, if any. `None` runs everything.
    pub fn config_path(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    /// How much of the document the operator asked for, on both axes.
    ///
    /// One accessor rather than two, because [`Presentation`] is the pair and
    /// nothing downstream ever wants one half of it. Building it through
    /// `From<View>` and then opting out is what keeps redaction the default here
    /// too: a branch that forgot `--raw` entirely would still produce the safe
    /// document.
    ///
    /// Both flags are named for what they *do*, the axes for what they *are*.
    /// Calling them `--complete` and `--redacted` would have argued for
    /// themselves: nobody wants an incomplete or a redacted picture of their
    /// server, so either would read as the obvious choice rather than as the
    /// costly one.
    pub fn presentation(&self) -> Presentation {
        let view = match self.include_volatile {
            true => View::Complete,
            false => View::Diffable,
        };

        match self.raw {
            true => Presentation::from(view).raw(),
            false => Presentation::from(view),
        }
    }

    /// Whether the caller said this binary is a temporary copy of itself.
    pub fn staged_binary(&self) -> bool {
        self.staged
    }

    /// Where the operator asked for the document, if anywhere in particular.
    pub fn output(&self) -> Option<&Path> {
        self.output.as_deref()
    }

    /// Whether the operator said an existing output file may be replaced.
    pub fn force(&self) -> bool {
        self.force
    }

    /// Whether to draw the live counter.
    ///
    /// Auto by default, so the contract that a clean redirected run says nothing on stderr
    /// holds by construction rather than by anybody remembering it.
    pub fn show_progress(&self, stderr_is_a_terminal: bool) -> bool {
        match (self.progress, self.no_progress) {
            (true, _) => true,
            (_, true) => false,
            _ => stderr_is_a_terminal,
        }
    }

    /// Whether the operator asked for a report of what the run cost.
    pub fn debug(&self) -> bool {
        self.debug
    }

    /// Whether the operator asked for every attribute rather than a digest of them.
    ///
    /// A `bool` rather than the `Detail` it selects, because this module knows nothing
    /// about collectors and `Detail` is one collector's vocabulary. The composition
    /// root maps it, which is the same shape as `--include-volatile` becoming a `View`
    /// except that `View` belongs to the document model and may be named here.
    pub fn full_detail(&self) -> bool {
        self.detail
    }
}

/// Reads the invocation, handling `--help` and `--version` on the way.
///
/// Exits the process on a malformed command line, which is clap's contract and
/// the right one: there is no fingerprint to emit if the request was not
/// understood.
pub fn parse() -> Cli {
    Cli::parse()
}
