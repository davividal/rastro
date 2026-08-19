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
//!   hang the run and a runaway one cannot exhaust the box's memory. Breaching either
//!   kills the tool's whole **process group**, not just the tool, so a helper it
//!   backgrounded does not outlive the failure. A descendant that puts itself in a new
//!   group with `setsid` escapes that, which is the one gap left and is not reachable by
//!   any tool rastro ships a collector for.
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
use subprocess::{Exec, ExecExt, ExitStatus, Job, JobExt, Redirection};

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

    /// Locates a tool, preferring paths known to be owned by the system.
    ///
    /// `resolve` searches `PATH`, and rastro runs as root. A directory on root's `PATH`
    /// that is not root-owned lets an attacker shadow a system tool, and rastro would
    /// then execute the plant with full privilege. Trying the well-known absolute paths
    /// first removes that in the ordinary case, and the `PATH` search stays as the wider,
    /// deliberately documented fallback so a tool installed somewhere unusual is still
    /// found rather than reported absent, which would be a lie.
    pub fn located(program: &str, candidates: &[&str]) -> Option<Self> {
        for candidate in candidates {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Some(Self {
                    program: program.to_owned(),
                    path,
                    limits: RunLimits::default(),
                });
            }
        }

        Self::resolve(program)
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

    /// Runs the tool and returns everything it wrote to stdout.
    pub fn run(&self, arguments: &[&str]) -> Result<String, CollectionError> {
        let mut job = self
            .command(arguments)
            .start()
            .map_err(|error| self.failure(format!("could not be started: {error}")))?;

        // Both run before either is propagated, so the child is always reaped, and the read's
        // own diagnosis wins over the wait's when both fail.
        let captured = self.capture(&mut job);
        let reaped = self.reap(&job);
        let (stdout, stderr) = captured?;
        let status = reaped?;

        if !status.success() {
            return Err(self.failure(format!(
                "exited unsuccessfully ({status:?}): {}",
                quoted_tail(&stderr)
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
            // Paired with the group kill below, which without this would signal rastro's own.
            .setpgid()
    }

    /// Waits for the tool, bounded, and kills its group if it outlasts the bound.
    ///
    /// The bounded read is not enough on its own. `Communicator::read` returns as soon as
    /// both pipes reach EOF, so a tool that writes its answer, closes its streams and keeps
    /// running satisfies the read and then blocks an unbounded `Job::wait` forever. The
    /// whole claim that a wedged tool cannot hang a run rests on this being `wait_timeout`.
    fn reap(&self, job: &Job) -> Result<ExitStatus, CollectionError> {
        let waited = job
            .wait_timeout(self.limits.time())
            .map_err(|error| self.failure(format!("could not be waited for: {error}")))?;

        if let Some(status) = waited {
            return Ok(status);
        }

        self.kill_the_group(job);

        // Reaped so nothing is left a zombie, then reported as the bound it broke rather than
        // as the signal: rastro sent that signal, so "exited unsuccessfully" would blame the
        // tool for rastro's own action.
        let _ = job.wait();

        Err(self.failure(format!("did not finish within {:?}", self.limits.time())))
    }

    /// Kills the tool and whatever it started.
    ///
    /// `Job::kill` signals only the pids it tracks, so a tool that backgrounds a helper
    /// would otherwise keep running unbounded after rastro had reported the failure and
    /// moved on. `JobExt::send_signal_group` is the crate's own answer, documented for
    /// exactly the pairing used here: a process started with `ExecExt::setpgid` has its whole
    /// group signalled.
    ///
    /// Signalled unconditionally, including when the direct child has already exited. That is
    /// the case the group kill exists for: a tool that backgrounds a helper and exits at once
    /// leaves the helper holding the pipes open, and an early return on "the child is gone"
    /// would spare exactly the descendant this is meant to reach. The pid cannot have been
    /// recycled while the group still has members, because each member holds the leader's
    /// `struct pid` as its group id, and a group with no members left simply yields `ESRCH`.
    fn kill_the_group(&self, job: &Job) {
        let _ = job.send_signal_group(libc::SIGKILL);
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
            self.kill_the_group(job);

            match error.kind() {
                ErrorKind::TimedOut => {
                    self.failure(format!("did not finish within {:?}", self.limits.time()))
                }
                _ => self.failure(format!("could not be read: {error}")),
            }
        })?;

        // The bound is on both streams together, which is how `limit_size` counts.
        if stdout.len() + stderr.len() > self.limits.output() {
            self.kill_the_group(job);

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

/// A failing tool's stderr, bounded and decoded without inventing characters.
///
/// Cut on a character boundary rather than a byte offset. Slicing mid-sequence and letting
/// `from_utf8_lossy` substitute `U+FFFD` is the very operation refused for stdout, and this
/// text reaches the document just the same.
fn quoted_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let boundary = text
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= STDERR_QUOTED)
        .last()
        .unwrap_or(0);

    text[..boundary].trim().to_owned()
}
