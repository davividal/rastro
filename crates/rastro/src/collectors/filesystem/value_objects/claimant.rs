//! Who decided what the walker does with a tree.

use std::fmt;

use rastro_collector::{ClaimQualifier, FacetName};

/// The facet that decided a tree's reading, and which of its entries asked.
///
/// Composed here rather than by the claimant, and that is the safety property: the facet
/// half comes from the collector's registered name, which a collector cannot choose for
/// itself, and only the qualifier comes from the claim. A collector can therefore mislabel
/// its own entry and can never file a decision under a peer's name.
///
/// `filesystem` and `config` are whole-facet claimants: rastro's own shipped rules and the
/// operator's have nothing below them to name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Claimant {
    facet: FacetName,
    entry: Option<ClaimQualifier>,
}

impl Claimant {
    /// A whole facet decided, with nothing below it to name.
    pub fn facet(facet: FacetName) -> Self {
        Self { facet, entry: None }
    }

    /// One of a facet's entries decided, keyed as that facet keys it.
    pub fn entry(facet: FacetName, entry: ClaimQualifier) -> Self {
        Self {
            facet,
            entry: Some(entry),
        }
    }
}

impl fmt::Display for Claimant {
    /// `postgresql:14/main`, or `nginx` where there was nothing to qualify.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.entry {
            Some(entry) => write!(formatter, "{}:{}", self.facet.as_str(), entry.as_str()),
            None => formatter.write_str(self.facet.as_str()),
        }
    }
}
