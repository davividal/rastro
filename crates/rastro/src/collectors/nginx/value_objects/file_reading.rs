//! What became of one configuration file.

use rastro_collector::{NonEmptyText, Observation, Xxh3Digest};

/// Whether a file was read, and if it was not, why not.
///
/// **A refusal is state, not a hole in the record.** A literal `include` of a file that is
/// not there stops nginx from starting at all, and a file the grammar refuses is one nginx
/// would refuse too. Both belong in the document beside the files that read cleanly, so a
/// box whose configuration will not survive its next reload says so in its fingerprint.
///
/// One failure variant rather than one per cause: the reason carries the distinction, and a
/// reader needs the sentence rather than the taxonomy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileReading {
    Parsed { digest: Xxh3Digest },
    Refused { reason: NonEmptyText },
}

impl From<&FileReading> for Observation {
    fn from(reading: &FileReading) -> Self {
        match reading {
            FileReading::Parsed { digest } => Observation::from(digest),
            FileReading::Refused { reason } => Observation::text(reason.as_str()),
        }
    }
}
