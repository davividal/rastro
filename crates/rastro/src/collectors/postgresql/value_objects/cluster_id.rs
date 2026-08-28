//! Which cluster on the box.

use std::cmp::Ordering;

use rastro_collector::{CollectionError, NonEmptyText};

/// A cluster's identity: its major version and its name, as one key.
///
/// **Both halves are needed, because neither is unique on its own.** Debian runs several
/// clusters side by side, and an upgrade is the ordinary way to get there: `16/main` and
/// `15/main` coexist for as long as the old data is kept, and two clusters of one version
/// (`16/main`, `16/staging`) are equally legal. Keying on version alone would let one
/// overwrite the other, and keying on name alone would do the same across versions.
///
/// Held as the rendered `version/name` rather than as the two parts, because that is the
/// whole of what this type is for: it is postgresql-common's own spelling, what
/// `pg_lsclusters` prints, and the facet key an operator reads. Each half is still validated
/// on the way in, so an empty one is refused rather than rendered as `17/` or `/main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterId(String);

impl ClusterId {
    pub fn new(
        version: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, CollectionError> {
        let version = NonEmptyText::new(version, "cluster version")?;
        let name = NonEmptyText::new(name, "cluster name")?;

        Ok(Self(format!("{}/{}", version.as_str(), name.as_str())))
    }

    /// The `version/name` form, which is also the facet key.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The major version as a number, where it reads as one.
    ///
    /// postgresql-common spells the version `17` on modern releases and `9.6` on the last of
    /// the old scheme, so the major is the integer before the first `.`. It is what decides
    /// which shape a version-dependent catalogue has, `pg_hba_file_rules` among them. `None`
    /// where the version is not a number rastro can read, which leaves the caller to take the
    /// conservative branch rather than guess.
    pub fn major_version(&self) -> Option<u32> {
        let version = self.0.split('/').next()?;

        version.split('.').next()?.parse::<u32>().ok()
    }
}

/// Ordered by the rendered key, so this type's order and the document's are the same order.
///
/// **Deliberately not numeric on the version, though `15/main` sorting before `9/legacy`
/// reads oddly.** The facet renders as an object, and an `Observation` object is a
/// `BTreeMap<String, _>`: the document's key order is lexicographic on the key text, decided
/// by the format rather than here. A numeric ordering on this type would therefore govern
/// the internal map and nothing a reader ever sees, leaving two orders that disagree and a
/// comment claiming an ordering the output does not have. Sorting by the same string the key
/// is makes them one order by construction.
///
/// Determinism, which is what the contract actually requires, holds either way: the same box
/// renders the same key order every run.
impl Ord for ClusterId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for ClusterId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
