//! Running a canonical tool as somebody else.
//!
//! rastro runs as root, so this drops privilege rather than gaining it, and that is the
//! only direction it can go: sudo from root to a service account needs no password and no
//! sudoers entry, while the reverse would need both.
//!
//! **Why it is needed at all.** A service's own state is often reachable only as the
//! account that owns it. A PostgreSQL cluster with the Debian default `local all all peer`
//! in `pg_hba.conf` refuses root outright, with `role "root" does not exist`, and the same
//! shape recurs: a user's crontab, a per-user container runtime, anything holding its
//! state behind an ownership check rather than behind file permissions.
//!
//! **What is kept, and what is given up.** Every guarantee of
//! [`CanonicalTool`](super::CanonicalTool) that matters is intact, because sudo *is* the
//! canonical tool here and the delegated program is one of its arguments: an absolute path
//! resolved before exec, an explicit argument vector rather than a command line, no shell,
//! bounded time and output, a process-group kill, and a non-zero exit reported rather than
//! partial output believed.
//!
//! One guarantee is genuinely weakened, and pretending otherwise would be worse than
//! saying so. **The environment is no longer empty.** sudo builds one for the target
//! account: `HOME`, `USER`, `LOGNAME`, `PATH`, `SHELL`, `MAIL`, `TERM` and its own
//! `SUDO_*` variables. Two consequences a collector author has to know:
//!
//! - `LC_ALL=C` survives only if the host's sudoers policy keeps it. Debian's default
//!   `env_reset` does keep `LC_*`, and that was confirmed on a Debian 12 cluster, but it is
//!   host configuration rather than something this seam can promise.
//! - `HOME` now points at the target account, so a tool that reads a per-user rc file will
//!   read *that account's*. psql is the immediate case: `~/.psqlrc` can change its output
//!   format, so the collector must pass `-X`. A tool with an rc file needs its own
//!   equivalent, and that belongs in the collector's source rather than here, because only
//!   the source knows the flag.

use rastro_collector::CollectionError;

use super::{CanonicalTool, RunLimits, TargetUser};

/// How rastro asks to become somebody else.
///
/// Located like any other tool, so a host without it reports absence rather than failing.
const DELEGATOR: &str = "sudo";

/// Refuse rather than prompt.
///
/// A fingerprint run has no operator in front of it. `CanonicalTool` already gives a tool
/// an immediate EOF on stdin, so a prompt could not be answered anyway; this turns the
/// silence into sudo's own diagnosis, which reaches the facet as its reason.
const NON_INTERACTIVE: &str = "-n";

/// A canonical tool, run as another account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAsUser {
    delegator: CanonicalTool,
    tool: CanonicalTool,
    user: TargetUser,
}

impl ToolAsUser {
    /// Locates both halves, or says why this host cannot run the tool that way.
    ///
    /// **A failure rather than an absence, and the distinction is the whole point.** A
    /// missing client says nothing about the service: a cluster listening on 5432 is
    /// running whether or not this host has a psql to talk to it with, and whether or not
    /// sudo is installed to become its owner. All rastro learns is that *it* cannot look,
    /// so the caller owes the facet an
    /// [`Undetermined`](rastro_collector::Presence::Undetermined) with this reason
    /// attached. Reporting `absent` would be a confident lie about the host.
    ///
    /// Contrast [`CanonicalTool::located`], where `None` genuinely is state: a box with no
    /// `dpkg-query` is a box that does not use dpkg, because there the tool *is* the
    /// subject rather than the way to reach it.
    ///
    /// The tool is checked before the delegator, so a host missing both is told about the
    /// one it would have to install first.
    pub fn located(program: &str, user: &TargetUser) -> Result<Self, CollectionError> {
        let tool = CanonicalTool::located(program).ok_or_else(|| {
            CollectionError::new(format!(
                "{program} is not installed in a system directory, so its state cannot be \
                 read as {}, only guessed at",
                user.as_str()
            ))
        })?;

        let delegator = CanonicalTool::located(DELEGATOR).ok_or_else(|| {
            CollectionError::new(format!(
                "{DELEGATOR} is not installed in a system directory, so {program} cannot be \
                 run as {}",
                user.as_str()
            ))
        })?;

        Ok(Self::using(delegator, tool, user.clone()))
    }

    /// The same, with both halves named by the caller.
    ///
    /// The escape hatch that mirrors [`CanonicalTool::located_in`]: a caller may say which
    /// binaries these are, and cannot weaken any guarantee by doing so.
    pub fn using(delegator: CanonicalTool, tool: CanonicalTool, user: TargetUser) -> Self {
        Self {
            delegator,
            tool,
            user,
        }
    }

    /// The same tool, held to different bounds.
    ///
    /// The bounds belong to the delegator, since that is the process rastro starts and the
    /// group it kills.
    pub fn within(mut self, limits: RunLimits) -> Self {
        self.delegator = self.delegator.within(limits);
        self
    }

    /// The delegated tool's name, which is what an operator recognises.
    pub fn program(&self) -> &str {
        self.tool.program()
    }

    pub fn user(&self) -> &TargetUser {
        &self.user
    }

    /// Runs the tool as the target user and returns everything it wrote to stdout.
    pub fn run(&self, arguments: &[&str]) -> Result<String, CollectionError> {
        let delegated = self.arguments_for(arguments);
        let borrowed: Vec<&str> = delegated.iter().map(String::as_str).collect();

        self.delegator.run(&borrowed).map_err(|failure| {
            CollectionError::new(format!(
                "{} as {}: {failure}",
                self.tool.program(),
                self.user.as_str()
            ))
        })
    }

    /// What sudo receives: the flags, the account, the tool's absolute path, then the
    /// tool's own arguments unchanged.
    ///
    /// Public because it is what a diagnosis quotes and what a test can check on a host
    /// where sudo is absent or would refuse.
    pub fn arguments_for(&self, arguments: &[&str]) -> Vec<String> {
        let mut delegated = vec![
            NON_INTERACTIVE.to_owned(),
            "-u".to_owned(),
            self.user.as_str().to_owned(),
            self.tool.path().display().to_string(),
        ];

        delegated.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        delegated
    }
}
