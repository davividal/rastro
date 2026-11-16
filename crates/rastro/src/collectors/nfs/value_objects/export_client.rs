//! Who an export is for.

use rastro_collector::{CollectionError, NonEmptyText};

/// The client an export names, as `exportfs` wrote it: a host, a network, a netgroup or `*`.
///
/// Kept as written rather than resolved, because the spelling is what mountd matches against
/// and resolving a name would be a network lookup this tool does not make.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExportClient(NonEmptyText);

impl ExportClient {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "export client")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
