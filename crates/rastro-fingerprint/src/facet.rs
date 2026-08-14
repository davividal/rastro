//! One collector's contribution to a document.

use crate::collector::{CollectorCategory, CollectorIdentity};
use crate::error::{FingerprintError, into_document_identifier};
use crate::observation::Observation;

/// Stable identifier of a state surface, such as `fs`, `processes` or `nginx`.
///
/// Identifies a facet within its fingerprint, and the same surface across two
/// fingerprints, which is what makes them comparable. Constrained to a
/// conservative character set because it keys the document, appears in diffs,
/// and will name files and command-line arguments later.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FacetName(String);

/// What a collector found, if anything.
///
/// Three variants rather than a status beside an optional payload: an absent
/// facet cannot accidentally carry data, and a failed one cannot pretend it
/// collected something. Variant names match the `ok | absent | error` values
/// that reach the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum FacetOutcome {
    /// The collector ran and found its subject.
    Ok { observation: Observation },
    /// The collector ran and its subject is not present on this host.
    ///
    /// Recorded, never omitted: absence is state.
    Absent,
    /// The collector failed. Loud in the output, never silent.
    Error { message: String },
}

/// One named section of a fingerprint, produced by exactly one collector.
#[derive(Debug, Clone, PartialEq)]
pub struct Facet {
    pub name: FacetName,
    pub collector: CollectorIdentity,
    pub category: CollectorCategory,
    pub outcome: FacetOutcome,
}

impl FacetName {
    pub fn new(value: impl Into<String>) -> Result<Self, FingerprintError> {
        Ok(Self(into_document_identifier(value.into(), "facet name")?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FacetOutcome {
    pub fn ok(observation: Observation) -> Self {
        Self::Ok { observation }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}

impl Facet {
    pub fn new(
        name: FacetName,
        collector: CollectorIdentity,
        category: CollectorCategory,
        outcome: FacetOutcome,
    ) -> Self {
        Self {
            name,
            collector,
            category,
            outcome,
        }
    }
}
