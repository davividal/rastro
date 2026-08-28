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
mod target_user;
mod tool_as_user;
mod tool_output;

pub use run_limits::RunLimits;
pub use target_user::TargetUser;
pub use tool_as_user::ToolAsUser;
pub use tool_output::ToolOutput;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use rastro_collector::CollectionError;
use subprocess::{Exec, ExecExt, ExitStatus, Job, JobExt, Redirection};

/// How much of a failing tool's stderr is quoted back.
const STDERR_QUOTED: usize = 512;

/// Linux `ETXTBSY`: a file still open for writing cannot be executed.
const TEXT_FILE_BUSY: i32 = 26;

/// How long a spawn keeps retrying while the binary is briefly held open for writing.
const SPAWN_BUSY_BUDGET: Duration = Duration::from_secs(1);

/// The pause between those retries; the condition clears in well under this.
const SPAWN_BUSY_PAUSE: Duration = Duration::from_millis(5);

/// What to say when a tool failed and said nothing, which is how many signal by status alone.
///
/// Something rather than nothing: the alternative reaches the document as a message ending in a
/// colon, which reads like text that went missing.
const NOTHING_ON_STDERR: &str = "and wrote nothing to stderr";

/// Where a system tool lives on the hosts rastro targets.
///
/// Owned here rather than passed in by each collector. A caller that had to supply the list
/// could supply the wrong one, or reach for the bare `PATH` search instead, and both mistakes
/// are silent. Five more shelling collectors are planned, so this is the difference between
/// one place to be right and six.
const SYSTEM_DIRECTORIES: [&str; 6] = [
    "/usr/bin",
    "/usr/sbin",
    "/bin",
    "/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];

/// How long a tool gets to die after `SIGKILL` before rastro stops waiting for it.
///
/// A signalled process normally goes in microseconds. One in uninterruptible sleep never goes
/// at all, and a hung NFS mount is exactly how that happens, which is a host state rastro's own
/// mount collector exists to surface. So even this wait is bounded.
const REAP_GRACE: Duration = Duration::from_secs(2);

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
    /// Locates a system tool, or reports that this host does not have it.
    ///
    /// Searches directories a distribution keeps root-owned, and nowhere else. rastro does not
    /// verify that ownership, so the list is a proxy for it rather than a check of it. What it
    /// buys is a set that is fixed and auditable, which root's inherited `PATH` is not.
    ///
    /// `None` is an ordinary answer rather than a failure: a box with no `dpkg-query` is a box
    /// that does not use dpkg, which is state.
    pub fn located(program: &str) -> Option<Self> {
        Self::located_in(program, &SYSTEM_DIRECTORIES)
    }

    /// The same, over directories the caller names, which is what [`Self::located`] is.
    ///
    /// There is no `PATH` fallback, and its absence is the point. An earlier version had one, on
    /// the argument that reporting a tool absent would be a lie about the host. That argument
    /// loses to this one: rastro runs as root, so searching root's inherited `PATH` lets the
    /// first directory on it that is not root-owned decide which binary runs with full
    /// privilege.
    ///
    /// Naming directories is the one guarantee a caller can widen. It cannot weaken the other
    /// five: whatever is found still runs from an absolute path, with no shell, a cleared
    /// environment, bounded time and output, and a group kill.
    pub fn located_in(program: &str, directories: &[&str]) -> Option<Self> {
        directories
            .iter()
            .map(|directory| Path::new(directory).join(program))
            .find(|path| path.is_file())
            .map(|path| Self::at(program, path))
    }

    fn at(program: &str, path: PathBuf) -> Self {
        Self {
            program: program.to_owned(),
            path,
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

    /// Runs the tool and returns everything it wrote to stdout.
    ///
    /// What almost every collector wants: the answer is on stdout and stderr is noise that
    /// only matters when the tool failed, in which case it is quoted into the error.
    pub fn run(&self, arguments: &[&str]) -> Result<String, CollectionError> {
        Ok(self.run_capturing_stderr(arguments)?.stdout)
    }

    /// The same run, with stderr kept.
    ///
    /// For the tools that answer on the wrong stream. `systemd_exporter --version` and
    /// `postgres_exporter --version` both print their version to stderr and exit zero, so a
    /// collector reading only stdout would report them as having no version rather than
    /// failing loudly — the quiet kind of wrong. Every guarantee this module makes is
    /// unchanged: same bounds, same group kill, same refusal of a non-zero exit and of
    /// invalid UTF-8.
    pub fn run_capturing_stderr(&self, arguments: &[&str]) -> Result<ToolOutput, CollectionError> {
        let mut job = self.started_tool(arguments)?;

        // Both run before either is propagated, so the child is always reaped, and the read's
        // own diagnosis wins over the wait's when both fail.
        let started = Instant::now();
        let captured = self.capture(&mut job, started);
        let reaped = self.reap(&job, started);
        let (stdout, stderr) = captured?;
        let status = reaped?;

        if !status.success() {
            return Err(self.failure(format!(
                "exited unsuccessfully ({status:?}): {}",
                quoted_tail(&stderr)
            )));
        }

        Ok(ToolOutput {
            stdout: self.decoded(stdout, "stdout")?,
            stderr: self.decoded(stderr, "stderr")?,
        })
    }

    /// Starts the tool, retrying only the transient `ETXTBSY`.
    ///
    /// A binary still held open for writing cannot be exec'd (`ETXTBSY`, "text file busy").
    /// rastro never writes the tools it runs, so on a real host this only appears when another
    /// process is mid-write to the binary, a package upgrade rewriting it, and it clears in
    /// milliseconds. A bounded retry turns that spurious failure into a successful run without
    /// relaxing any guarantee: the resolved path, argument vector, cleared environment and run
    /// bounds are identical on every attempt.
    fn started_tool(&self, arguments: &[&str]) -> Result<Job, CollectionError> {
        let deadline = Instant::now() + SPAWN_BUSY_BUDGET;
        loop {
            match self.command(arguments).start() {
                Ok(job) => return Ok(job),
                Err(error)
                    if error.raw_os_error() == Some(TEXT_FILE_BUSY)
                        && Instant::now() < deadline =>
                {
                    thread::sleep(SPAWN_BUSY_PAUSE);
                }
                Err(error) => {
                    return Err(self.failure(format!("could not be started: {error}")));
                }
            }
        }
    }

    /// One stream, refused rather than repaired when it is not UTF-8.
    fn decoded(&self, stream: Vec<u8>, which: &str) -> Result<String, CollectionError> {
        String::from_utf8(stream)
            .map_err(|_| self.failure(format!("wrote {which} that is not valid UTF-8")))
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
    fn reap(&self, job: &Job, started: Instant) -> Result<ExitStatus, CollectionError> {
        // An error here, not only a timeout, has to reach `detach` below: `Job`'s own `drop`
        // waits untimed, so returning early would put the unbounded wait straight back.
        if let Ok(Some(status)) = job.wait_timeout(self.remaining(started)) {
            return Ok(status);
        }

        self.kill_the_group(job);

        // Bounded too, and it gives up rather than becoming the hang it exists to prevent.
        if job.wait_timeout(REAP_GRACE).ok().flatten().is_none() {
            job.detach();
        }

        // Reported as the bound it broke, not as the signal rastro itself sent.
        Err(self.failure(format!("did not finish within {:?}", self.limits.time())))
    }

    /// What is left of the run's bound.
    fn remaining(&self, started: Instant) -> Duration {
        self.limits.time().saturating_sub(started.elapsed())
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
    ///
    /// One dependency worth naming because nothing enforces it: `send_signal_group` no-ops once
    /// `subprocess` has cached an exit status, so this only helps because neither the read nor
    /// the wait calls `wait` or `poll` before it. Adding such a call ahead of this would
    /// silently restore the leak, and `run_kills_a_descendant_of_a_tool_that_already_exited`
    /// is what would catch it.
    fn kill_the_group(&self, job: &Job) {
        let _ = job.send_signal_group(libc::SIGKILL);
    }

    /// Reads both streams under the bounds, killing the child if either is breached.
    ///
    /// A tool still running when the clock runs out is killed here rather than left
    /// behind: `subprocess` deliberately keeps the child alive so that reading can
    /// resume, which is not what rastro wants from a tool that has already hung.
    fn capture(
        &self,
        job: &mut Job,
        started: Instant,
    ) -> Result<(Vec<u8>, Vec<u8>), CollectionError> {
        let outcome = job
            .communicate()
            .map_err(|error| self.failure(format!("could not be communicated with: {error}")))?
            .limit_time(self.remaining(started))
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

/// The end of a failing tool's stderr, bounded and cut on a character boundary.
///
/// The **end**, which is what the name says and what the previous version did not do. A tool
/// writes its warnings first and its fatal line last, so truncating from the front keeps the
/// noise and discards the reason.
///
/// Cut on a character boundary rather than a byte offset, because slicing mid-sequence and
/// letting `from_utf8_lossy` substitute `U+FFFD` is the operation this module refuses for stdout,
/// and this text reaches the document just the same. Lossy decoding still applies to input that
/// was already invalid, which is a different thing from making it invalid by cutting.
fn quoted_tail(stderr: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(stderr);

    // Trimmed before the bound is applied, not after. A tool that writes its reason and then six
    // hundred newlines would otherwise have the whole reason cut away and a screen of blank space
    // quoted in its place.
    let text = lossy.trim();

    // Measured from the end, so a message already within the bound comes back whole. Comparing
    // start indices against the bound instead dropped the last character of every message, and
    // the whole of a one-character one.
    if text.is_empty() {
        return NOTHING_ON_STDERR.to_owned();
    }

    let start = text
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| text.len() - index <= STDERR_QUOTED)
        .unwrap_or(0);

    text[start..].to_owned()
}
