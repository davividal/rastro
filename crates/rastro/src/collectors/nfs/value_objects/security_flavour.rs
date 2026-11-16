//! An RPC security flavour an export accepts.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// `sys`, `krb5`, `krb5i`, `krb5p` or `none`, as `exportfs` wrote it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SecurityFlavour(NonEmptyText);

impl SecurityFlavour {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "security flavour")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SecurityFlavour> for Observation {
    fn from(flavour: &SecurityFlavour) -> Self {
        Observation::text(flavour.as_str())
    }
}
