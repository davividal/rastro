//! A subtree of the filesystem, as a policy names it.

use rastro_collector::{AbsolutePath, CollectionError};

/// The root of a subtree a policy rule applies to.
///
/// Absolute, and spelled without a trailing separator, so `/var/log/` and `/var/log`
/// are one tree rather than two. Normalising here rather than at every comparison is
/// what stops a rule that reads correctly from matching nothing at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WalkedTree(AbsolutePath);

impl WalkedTree {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let path = AbsolutePath::new(value, "walked tree")?;
        let trimmed = path.as_str().trim_end_matches('/');

        // The trim empties only the root, and one separator is what the root is stored
        // as: `///` kept as spelled is a depth-zero tree no walked path matches.
        let normalised = if trimmed.is_empty() { "/" } else { trimmed };

        Ok(Self(AbsolutePath::new(normalised, "walked tree")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether a path lies in this tree, the tree's own directory included.
    ///
    /// Component-aware, and that is the whole point: `/var/log` does not contain
    /// `/var/logrotate.conf`, though a comparison of the text says it does. Getting
    /// this wrong stops a neighbouring tree being hashed, and nothing in the document
    /// would say so.
    pub fn contains(&self, path: &AbsolutePath) -> bool {
        if self.is_root() {
            return true;
        }

        let candidate = path.as_str();
        let tree = self.as_str();

        candidate == tree || candidate.starts_with(&format!("{tree}/"))
    }

    /// How deep the tree sits, so the most specific rule for a path can be found.
    pub fn depth(&self) -> usize {
        self.as_str()
            .split('/')
            .filter(|component| !component.is_empty())
            .count()
    }

    /// Whether this is the tree that contains every path.
    ///
    /// The one question a table must be able to answer about itself, and asking it here
    /// rather than reading a depth of zero off the text keeps the constructor's totality
    /// check and [`Self::contains`] on the same notion of root.
    pub fn is_root(&self) -> bool {
        self.as_str() == "/"
    }
}
