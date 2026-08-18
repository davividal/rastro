//! Running the tool that already knows the answer.
//!
//! Where parsing a canonical tool's output is more honest than reimplementing what it
//! does, a collector's source shells out. This is the one place that does it, and the
//! one place the hardening lives, because rastro runs as root on production servers
//! and a fingerprint run must not be able to take one down.
//!
//! What is guaranteed here, and why each matters:
//!
//! - **An absolute path, resolved before exec.** The tool is located once, when the
//!   collector is constructed, and that exact path is what runs. A bare program name
//!   handed to `exec` resolves against `PATH` at the moment of the call, so what was
//!   detected and what ran could differ.
//! - **No shell.** An explicit argument vector, never a command line. Nothing from the
//!   config or the command line reaches an argument: every argument in this binary is a
//!   literal written by a collector author.
//! - **A cleared environment, plus `LC_ALL=C`.** Both hardening and determinism: a
//!   localised box would otherwise render different bytes for the same state, and an
//!   inherited environment is an input nobody audited.
//! - **Immediate end of input.** A tool that decides to prompt gets EOF rather than a
//!   terminal, so it cannot wait for an operator who is not there.
//! - **A time bound and an output bound**, from [`RunLimits`]. A wedged tool cannot
//!   hang the run and a runaway one cannot exhaust the box's memory.
//! - **Exit status success, or nothing.** A non-zero exit is a failure carrying its
//!   status and a bounded tail of stderr, never partial output treated as an answer.
//! - **Strictly valid UTF-8.** Invalid bytes are refused rather than replaced with
//!   `U+FFFD`, which is why this reads bytes rather than calling `read_string`.
//!   Substituting a character would put text into a fingerprint that was never on the
//!   box.
//!
//! Every failure is a [`CollectionError`], so the facet is recorded as `error` and the
//! run continues. One unavailable tool never costs the whole document.
//!
//! **On the output bound, and why it is read one byte past.** `subprocess` enforces
//! `limit_size` by *stopping* the read and buffering the remainder, not by failing. Used
//! as it comes, that would hand rastro a quietly truncated answer, which is the exact
//! bug that disqualified configsnap and prompted this tool (see `docs/research.md`). It
//! would also wedge the wait, because a child whose pipe has filled blocks forever.
//! So the limit is set one byte higher than the bound and anything past the bound is a
//! recorded failure with the child killed.

mod run_limits;

pub use run_limits::RunLimits;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;
use subprocess::{Exec, Job, Redirection};

/// How much of a failing tool's stderr is quoted back.
const STDERR_QUOTED: usize = 512;

/// A tool found on this host, ready to run.
///
/// Holding the resolved path is the point: construction is the detection, so a
/// collector's `presence` and its `collect` cannot disagree about which binary they are
/// talking about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTool {
    program: String,
    path: PathBuf,
    limits: RunLimits,
}

impl CanonicalTool {
    /// Locates a tool by name, or reports that this host does not have it.
    ///
    /// `None` is an ordinary answer rather than a failure: a box with no `dpkg-query` is
    /// a box that does not use dpkg, which is state, not an error.
    pub fn resolve(program: &str) -> Option<Self> {
        which::which(program).ok().map(|path| Self {
            program: program.to_owned(),
            path,
            limits: RunLimits::default(),
        })
    }

    /// A tool at a path chosen by the caller.
    pub fn at(program: &str, path: impl Into<PathBuf>) -> Self {
        Self {
            program: program.to_owned(),
            path: path.into(),
            limits: RunLimits::default(),
        }
    }

    /// The same tool, held to different bounds.
    pub fn within(mut self, limits: RunLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn limits(&self) -> RunLimits {
        self.limits
    }

    /// Runs the tool and returns everything it wrote to stdout.
    pub fn run(&self, arguments: &[&str]) -> Result<String, CollectionError> {
        let mut job = self
            .command(arguments)
            .start()
            .map_err(|error| self.failure(format!("could not be started: {error}")))?;

        // Read first, then wait unconditionally, so a child this killed is reaped
        // rather than left as a zombie for the rest of the run.
        let captured = self.capture(&mut job);
        let status = job
            .wait()
            .map_err(|error| self.failure(format!("could not be waited for: {error}")))?;
        let (stdout, stderr) = captured?;

        if !status.success() {
            return Err(self.failure(format!(
                "exited unsuccessfully ({status:?}): {}",
                String::from_utf8_lossy(&stderr[..stderr.len().min(STDERR_QUOTED)]).trim()
            )));
        }

        String::from_utf8(stdout)
            .map_err(|_| self.failure("wrote output that is not valid UTF-8".to_owned()))
    }

    /// The resolved path, an explicit argument vector, one environment variable, and
    /// empty input so a prompt becomes an immediate EOF.
    fn command(&self, arguments: &[&str]) -> Exec {
        Exec::cmd(self.path.clone())
            .args(arguments.iter().copied())
            .env_clear()
            .env("LC_ALL", "C")
            .stdin("")
            .stdout(Redirection::Pipe)
            .stderr(Redirection::Pipe)
    }

    /// Reads both streams under the bounds, killing the child if either is breached.
    ///
    /// A tool still running when the clock runs out is killed here rather than left
    /// behind: `subprocess` deliberately keeps the child alive so that reading can
    /// resume, which is not what rastro wants from a tool that has already hung.
    fn capture(&self, job: &mut Job) -> Result<(Vec<u8>, Vec<u8>), CollectionError> {
        let outcome = job
            .communicate()
            .map_err(|error| self.failure(format!("could not be communicated with: {error}")))?
            .limit_time(self.limits.time())
            .limit_size(self.limits.output().saturating_add(1))
            .read();

        let (stdout, stderr) = outcome.map_err(|error| {
            let _ = job.kill();

            match error.kind() {
                ErrorKind::TimedOut => self.failure(format!(
                    "did not finish within {} seconds",
                    self.limits.time().as_secs()
                )),
                _ => self.failure(format!("could not be read: {error}")),
            }
        })?;

        // The bound is on both streams together, which is how `limit_size` counts.
        if stdout.len() + stderr.len() > self.limits.output() {
            let _ = job.kill();

            return Err(self.failure(format!(
                "wrote more than {} bytes, so its answer would have been truncated",
                self.limits.output()
            )));
        }

        Ok((stdout, stderr))
    }

    /// Every failure names the tool and its resolved path, so an operator reading
    /// stderr knows which binary on their box misbehaved.
    fn failure(&self, reason: String) -> CollectionError {
        CollectionError::new(format!(
            "{} ({}) {reason}",
            self.program,
            self.path.display()
        ))
    }
}
