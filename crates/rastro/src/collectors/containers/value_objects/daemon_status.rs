//! Whether the engine's control plane answered.

use rastro_collector::{NonEmptyText, Observation};

/// Whether the thing that knows about the containers is reachable, and why not when it is
/// not.
///
/// **An engine installed with nothing answering is state, not a failed read.** A box with
/// docker on it and a stopped daemon has been set up for containers and is not running any,
/// which is a different fact from having no docker at all, and a different fact again from
/// rastro being unable to look. So this reaches the document as an observation, while the
/// reasons rastro genuinely could not tell surface as a facet `error`.
///
/// The reason is optional because a tool is free to fail and say nothing. Recording an empty
/// string instead would claim the engine explained itself and had nothing to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonStatus {
    Answering,
    Unreachable { reason: Option<NonEmptyText> },
}

impl DaemonStatus {
    /// Unreachable, with whatever the tool complained about.
    ///
    /// Trimmed because the complaint arrives on stderr with the trailing newline a tool
    /// writes, and a trailing newline in a document is a difference between two runs that
    /// says nothing about the box.
    pub fn unreachable(reason: &str) -> Self {
        Self::Unreachable {
            reason: NonEmptyText::new(reason.trim(), "unreachable daemon reason").ok(),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Answering => "answering",
            Self::Unreachable { .. } => "unreachable",
        }
    }

    /// Why the control plane did not answer, when it said.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Answering => None,
            Self::Unreachable { reason } => reason.as_ref().map(NonEmptyText::as_str),
        }
    }
}

impl From<&DaemonStatus> for Observation {
    fn from(status: &DaemonStatus) -> Self {
        Observation::text(status.as_str())
    }
}
