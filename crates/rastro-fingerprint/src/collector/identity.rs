//! Naming a collector, and the version of it that ran.

use crate::error::{FingerprintError, into_document_identifier};

/// Identifier of the collector that produced a facet.
///
/// Distinct from a facet name: what was observed is not who observed it. One
/// collector may produce differently named facets, and an exec collector's id
/// outlives the facet names it currently emits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CollectorId(String);

/// Version of a collector, recorded per facet.
///
/// Deliberately not parsed as a semantic version: exec collectors are versioned
/// by whatever their authors use, including git revisions and dates. The only
/// rule is that it must be a single printable token.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CollectorVersion(String);

/// Which collector produced a facet, and which version of it.
///
/// Recorded per facet rather than once per run, because exec collectors are
/// versioned independently of the rastro binary that invoked them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorIdentity {
    pub id: CollectorId,
    pub version: CollectorVersion,
}

impl CollectorId {
    pub fn new(value: impl Into<String>) -> Result<Self, FingerprintError> {
        Ok(Self(into_document_identifier(
            value.into(),
            "collector id",
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CollectorVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, FingerprintError> {
        let value = value.into();
        const KIND: &str = "collector version";

        if value.is_empty() {
            return Err(FingerprintError::EmptyIdentifier { kind: KIND });
        }
        if value.chars().any(char::is_whitespace) {
            return Err(FingerprintError::WhitespaceInIdentifier { kind: KIND, value });
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CollectorIdentity {
    pub fn new(id: CollectorId, version: CollectorVersion) -> Self {
        Self { id, version }
    }
}
