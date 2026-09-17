//! Which of a claimant's own subjects a claim was made for.

use crate::CollectionError;

/// The separator between a facet's name and this qualifier, once the two are composed.
///
/// Reserved rather than escaped: a name a person reads in order to look an entry up is worth
/// more than the freedom to put a colon in a cluster name.
const SEPARATOR: char = ':';

/// The entry a claim was made on behalf of, keyed as its own facet keys it.
///
/// A facet with one subject has nothing to qualify. A facet that keys several needs one,
/// because "postgresql claims this tree" does not say which cluster, and two clusters
/// pointed at one directory is precisely the state worth reading.
///
/// **It qualifies the claimant, it does not name one.** A claim still carries no facet name,
/// for the reason [`FilesystemClaim`](super::FilesystemClaim) gives: a collector naming
/// itself could name somebody else. The gatherer supplies the facet half, so the worst a
/// mistaken collector can do here is mislabel its own entry.
///
/// The value is the key that entry has in its own facet's `data`, so `postgresql:14/main`
/// leads a reader straight to it rather than approximately near it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClaimQualifier(String);

impl ClaimQualifier {
    /// Reads a qualifier, or refuses one that cannot be composed or followed.
    ///
    /// Two refusals. An empty one composes to a name with trailing punctuation and points at
    /// no entry. One holding the separator leaves the composed name unable to say where the
    /// facet's half ended.
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let value = value.into();

        if value.is_empty() {
            return Err(CollectionError::new(
                "a claim qualifier names the entry that asked, so it cannot be empty",
            ));
        }

        if value.contains(SEPARATOR) {
            return Err(CollectionError::new(format!(
                "the claim qualifier {value:?} holds {SEPARATOR:?}, which is what joins it to \
                 the facet name, so the composed claimant could not be read back"
            )));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
