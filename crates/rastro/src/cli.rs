//! The command line as the operator meets it.

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
}

impl Cli {
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
}

/// Reads the invocation, handling `--help` and `--version` on the way.
///
/// Exits the process on a malformed command line, which is clap's contract and
/// the right one: there is no fingerprint to emit if the request was not
/// understood.
pub fn parse() -> Cli {
    Cli::parse()
}
