//! What a tool is allowed to consume.

use std::time::Duration;

/// The bounds one run of a canonical tool is held to.
///
/// A value rather than two constants, because the right bound is a property of the
/// tool: `dpkg-query` answers in about a second, while a Layer 3 dump of a large
/// database legitimately takes longer. Making it explicit means raising a bound is a
/// visible decision at the call site rather than an edit to a global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    time: Duration,
    output: usize,
}

/// Generous next to what these tools actually cost, and still bounded. The point is
/// that no value of "hung" stops a fingerprint from completing.
const DEFAULT_TIME: Duration = Duration::from_secs(30);

/// Two orders of magnitude above the largest real answer, a package list on a
/// well-stocked box being a few megabytes.
const DEFAULT_OUTPUT: usize = 64 * 1024 * 1024;

impl RunLimits {
    pub fn new(time: Duration, output: usize) -> Self {
        Self { time, output }
    }

    pub fn time(&self) -> Duration {
        self.time
    }

    /// The most a tool may write across both streams together.
    pub fn output(&self) -> usize {
        self.output
    }
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            time: DEFAULT_TIME,
            output: DEFAULT_OUTPUT,
        }
    }
}
