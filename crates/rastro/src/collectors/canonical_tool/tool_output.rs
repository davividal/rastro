//! What a tool wrote, both streams.

/// The output of one successful run.
///
/// **Both streams, because a tool's answer is not always on stdout.** Of the five
/// Prometheus-style exporters on the development box, `node_exporter` and
/// `process-exporter` print `--version` to stdout while `systemd_exporter` and
/// `postgres_exporter` print the same text to stderr, all four exiting zero. A collector
/// that read stdout alone would report those two as having no version at all, which is
/// wrong rather than merely incomplete.
///
/// Reading stderr is not the same as trusting it: a *failing* tool's stderr is still
/// quoted back as a diagnostic and never treated as an answer. This type only exists for
/// runs that succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub stdout: String,
    pub stderr: String,
}
