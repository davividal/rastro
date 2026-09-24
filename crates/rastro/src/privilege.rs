//! Whether this run can read what only root may read, said before it starts.
//!
//! **A warning, not a refusal.** rastro is designed to run as root and is run unprivileged
//! often enough for that to be ordinary: what it cannot read becomes an `error` in the
//! document rather than a guess, which is honest but easy to miss. Said before the run
//! because that is when the operator can still rerun it with `sudo` at no cost. Which facets
//! will fail is not predicted here, since that depends on the box; the summary after the run
//! names them.

use std::fs;

/// Where the kernel reports the ids this process runs with.
const PROC_SELF_STATUS: &str = "/proc/self/status";

/// The effective uid, the one access checks use, or nothing if it cannot be read.
pub fn effective_user_id() -> Option<u32> {
    effective_user_id_in(&fs::read_to_string(PROC_SELF_STATUS).ok()?)
}

/// The same over a status text the caller supplies.
///
/// The `Uid:` line carries real, effective, saved and filesystem ids in that order. The
/// effective one is the answer: a setuid wrapper leaves the real id as the caller's.
pub fn effective_user_id_in(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// The warning for a run that is not root, or nothing.
///
/// Nothing too when the id could not be read, rather than a guess that would warn root about
/// a problem it does not have.
pub fn concern(effective_user_id: Option<u32>) -> Option<String> {
    match effective_user_id? {
        0 => None,
        uid => Some(format!(
            "running as uid {uid}, not root: what only root may read is recorded as an error \
             rather than read, so this fingerprint will be incomplete; run it with sudo for \
             the whole box"
        )),
    }
}
