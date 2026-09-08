//! `ctr --version` and `ctr version`: containerd's spelling of who is speaking.

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::value_objects::EngineVersion;

/// The word beginning the block about the engine that answered.
const SERVER: &str = "Server:";

const VERSION_KEY: &str = "Version:";
const REVISION_KEY: &str = "Revision:";

/// The client's own version, from `ctr --version`.
///
/// **A different read from the `version` subcommand, and measured to be the only one that
/// works.** `ctr version` reports both halves but has to reach the socket to do it: against
/// an address with nothing behind it, containerd 2.3.4's `ctr` exits non-zero and prints
/// *nothing at all*, not even the client's half. `ctr --version` never connects, so it
/// answers on a box where containerd is installed and stopped — which is exactly the box
/// whose state this facet has to be able to describe.
///
/// The line is `ctr github.com/containerd/containerd/v2 v2.3.4`, and the version is its last
/// word.
pub struct CtrClientVersion;

impl CtrClientVersion {
    pub fn parse(output: &str) -> Result<EngineVersion, CollectionError> {
        let reported = output
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().last())
            .ok_or_else(|| {
                CollectionError::new(
                    "could not read what `ctr --version` reported: no version in it",
                )
            })?;

        EngineVersion::new(reported)
    }
}

/// What the answering containerd said about itself, from the `Server:` block of
/// `ctr version`.
///
/// **Text, because `ctr` offers no machine-readable form of this.** Every other read of
/// containerd here uses `--quiet` or the JSON of `containers info`; the version is the one
/// place with neither. `ctr` calls itself a debug tool and promises nothing about its
/// output, which is why output this cannot read is a recorded failure rather than an engine
/// with no version: a forgiving parse would hide the day the format changed.
pub struct CtrServerVersions {
    pub version: EngineVersion,
    pub revision: NonEmptyText,
}

impl CtrServerVersions {
    /// The server's half, refusing output that does not carry it.
    ///
    /// **A missing server block is a failure rather than an absence, and that follows from a
    /// measurement.** A `ctr` that cannot reach containerd exits non-zero and prints
    /// nothing, which the execution seam turns into a failure before this is called. So a
    /// *successful* `ctr version` with no server block in it does not mean nothing answered:
    /// it means the output is not the shape rastro reads, which is exactly the day this
    /// needs to be loud.
    pub fn parse(output: &str) -> Result<Self, CollectionError> {
        let block = block(output, SERVER);

        let (Some(version), Some(revision)) =
            (value(&block, VERSION_KEY), value(&block, REVISION_KEY))
        else {
            return Err(CollectionError::new(
                "could not read what `ctr version` reported: no server version and revision \
                 in it",
            ));
        };

        Ok(Self {
            version: EngineVersion::new(version)?,
            revision: NonEmptyText::new(revision, "containerd revision")?,
        })
    }
}

/// The lines under one heading, up to the next one.
///
/// The heading sits alone at the start of a line and every line of its block is indented,
/// which is what makes this a split rather than a parse.
fn block(output: &str, heading: &str) -> Vec<String> {
    output
        .lines()
        .skip_while(|line| line.trim() != heading)
        .skip(1)
        .take_while(|line| line.starts_with(char::is_whitespace) || line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// The value of one `Key: value` line inside a block.
fn value(block: &[String], key: &str) -> Option<String> {
    block
        .iter()
        .find_map(|line| line.trim().strip_prefix(key))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
