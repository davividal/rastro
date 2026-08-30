//! The command line as the operator meets it.

use std::path::{Path, PathBuf};

use clap::Parser;

use rastro_fingerprint::View;

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

    /// Record every attribute of every walked path, rather than one digest of them.
    ///
    /// The default records a digest per path, which answers what a fingerprint is
    /// taken to answer — did anything about this path change — and costs a fifth
    /// of the document. This asks *which* attribute moved, and it has to be asked
    /// at the time: a summary taken yesterday cannot be expanded today.
    #[arg(long)]
    detail: bool,
}

impl Cli {
    /// The config file the operator named, if any. `None` runs everything.
    pub fn config_path(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    /// Which view the operator asked for.
    ///
    /// The flag is named for what it *does*, the view for what it *is*. Calling
    /// the flag `--complete` would have argued for itself: nobody wants an
    /// incomplete picture of their server, so it would read as the obvious
    /// choice rather than as the noisy one.
    pub fn view(&self) -> View {
        if self.include_volatile {
            View::Complete
        } else {
            View::Diffable
        }
    }

    /// Whether the caller said this binary is a temporary copy of itself.
    pub fn staged_binary(&self) -> bool {
        self.staged
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
