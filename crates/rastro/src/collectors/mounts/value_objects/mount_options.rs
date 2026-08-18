//! Everything a mount was mounted with, as the set it is.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::mount_option::MountOption;

/// The options of one mount.
///
/// A set, not a list, so ordering is a property of the type rather than of a `sort`
/// call somebody has to remember. The host's own order for a set of flags is
/// arbitrary churn and would otherwise reach the diff.
///
/// It orders *decoded* options, because the source decodes before constructing.
/// Ordering escaped text would sort by an artefact of the transport encoding:
/// `a\040b` sorts after `aB` escaped, since `\` is 0x5C and `B` is 0x42, and
/// before it decoded, since a space is 0x20.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MountOptions(BTreeSet<MountOption>);

impl MountOptions {
    pub fn new(options: impl IntoIterator<Item = MountOption>) -> Self {
        Self(options.into_iter().collect())
    }

    /// The options in order, which is the sorted order of their decoded text.
    pub fn iter(&self) -> impl Iterator<Item = &MountOption> {
        self.0.iter()
    }
}

impl From<&MountOptions> for Observation {
    fn from(options: &MountOptions) -> Self {
        Observation::list(
            options
                .iter()
                .map(|option| Observation::text(option.as_str())),
        )
    }
}
