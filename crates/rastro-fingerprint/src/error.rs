//! Why a value was refused entry to the model.

use thiserror::Error;

/// A fingerprint could not be built from the values offered.
///
/// Every variant names the rule that was broken rather than the field that
/// broke it, because the same rule guards identifiers across several modules.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FingerprintError {
    #[error("a {kind} must not be empty")]
    EmptyIdentifier { kind: &'static str },

    #[error("a {kind} may contain only lowercase letters, digits, '-' and '_', got {value:?}")]
    MalformedIdentifier { kind: &'static str, value: String },

    #[error("a {kind} must not contain whitespace, got {value:?}")]
    WhitespaceInIdentifier { kind: &'static str, value: String },

    #[error(
        "two facets share the name {name:?}, so one state surface would silently shadow another"
    )]
    DuplicateFacetName { name: String },
}

/// The character set shared by every identifier that keys a document.
///
/// Kept narrow on purpose: these names end up in diffs, in file names and on
/// the command line, so anything needing quoting or escaping is refused at the
/// door rather than handled everywhere downstream.
pub(crate) fn into_document_identifier(
    value: String,
    kind: &'static str,
) -> Result<String, FingerprintError> {
    if value.is_empty() {
        return Err(FingerprintError::EmptyIdentifier { kind });
    }

    let is_legal = value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
    });

    if is_legal {
        Ok(value)
    } else {
        Err(FingerprintError::MalformedIdentifier { kind, value })
    }
}
