//! One option of an export.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A flag such as `root_squash`, or a setting such as `fsid=7`, as `exportfs` wrote it.
///
/// Not decoded further: the vocabulary belongs to nfs-utils and grows with it, and an option
/// rastro had no name for would otherwise have to be dropped or refused.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExportOption(NonEmptyText);

impl ExportOption {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "export option")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ExportOption> for Observation {
    fn from(option: &ExportOption) -> Self {
        Observation::text(option.as_str())
    }
}
