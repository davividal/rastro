//! What a tool wrote, and whether it said it succeeded, for a caller that decides for itself.

/// Both streams of one run and whether it exited zero.
///
/// **For the one tool whose exit status does not say whether the read failed.** `docker
/// version` with no daemon to answer exits 1 on 29.5.3, measured, and 0 on other releases,
/// and either way prints the client's own document on stdout with `"Server": null`. That is
/// an answer about the box, so the caller parses before it judges; every other tool goes
/// through [`CanonicalTool::run_capturing_stderr`](super::CanonicalTool), which refuses a
/// non-zero exit outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExit {
    pub succeeded: bool,
    pub stdout: String,
    pub stderr: String,
}
