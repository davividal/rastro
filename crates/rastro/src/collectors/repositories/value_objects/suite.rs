//! Which release of a repository is being tracked.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A suite, which apt also calls a distribution.
///
/// `bookworm`, `bookworm-security`, `bookworm-pgdg`. Which one a box tracks is among
/// the most consequential lines in its configuration, since moving from `bookworm` to
/// `trixie` is a whole-distribution upgrade expressed as one word.
///
/// **A suite ending in `/` is a path, not a release name, and that is legal.** apt
/// calls it a flat repository: `deb https://example.org/repo ./` serves packages from
/// one directory with no `dists` hierarchy and no components at all. The value is kept
/// as written, because the trailing slash is precisely what tells apt which layout to
/// expect.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Suite(NonEmptyText);

impl Suite {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "suite")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this names a flat repository rather than a release.
    pub fn is_a_flat_repository(&self) -> bool {
        self.as_str().ends_with('/')
    }
}

impl From<&Suite> for Observation {
    fn from(suite: &Suite) -> Self {
        Observation::text(suite.as_str())
    }
}
