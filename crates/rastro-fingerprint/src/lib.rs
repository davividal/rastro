//! What a fingerprint *is*, and its canonical form on the wire.
//!
//! Nothing here performs I/O, spawns a process or reads `/proc`. That is not
//! tidiness: it is what lets the whole model be built and tested on a machine
//! that is not the target platform, and it is enforced by this crate's
//! dependency list rather than by anyone's discipline.
//!
//! The modules are the model's joints:
//!
//! - [`facet`]: one collector's contribution, and the name that identifies it.
//! - [`observation`]: what was seen, and what the seer asserted about it.
//! - [`collector`]: which collector produced a facet. Identity only; the
//!   contract a collector fulfils lives in the `rastro-collector` crate.
//! - [`view`]: which part of a document is being asked for.
//! - [`error`]: why a value was refused entry to the model.
//! - [`json`]: the wire shape, and the only place that knows it.

pub mod collector;
pub mod error;
pub mod facet;
pub mod json;
pub mod observation;
pub mod view;

pub use collector::{CollectorCategory, CollectorId, CollectorIdentity, CollectorVersion};
pub use error::FingerprintError;
pub use facet::{Facet, FacetName, FacetOutcome};
pub use observation::{
    Content, Observation, Scalar, Sensitivity, Visible, VisibleContent, VisibleList, VisibleObject,
    Volatility,
};
pub use view::View;

use crate::collector::CollectorCategory as Category;
use crate::error::FingerprintError as Error;

/// Version of the output format contract.
///
/// Rises whenever a change would break a consumer that reads the previous
/// shape. It will do so several times before v1 ships, which is the mechanism
/// working rather than failing: a consumer written against an earlier shape can
/// tell, instead of silently misreading a document.
pub const SCHEMA_VERSION: u32 = 1;

/// A complete fingerprint of one host at one moment.
///
/// The consistency boundary: facet names identify state surfaces, so a
/// duplicate would let one surface silently shadow another and the document
/// could not be lined up against a second run. Constructed valid or not at all.
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    facets: Vec<Facet>,
}

impl Fingerprint {
    pub fn from_facets(facets: impl IntoIterator<Item = Facet>) -> Result<Self, Error> {
        let mut facets: Vec<Facet> = facets.into_iter().collect();
        facets.sort_by(|left, right| left.name.cmp(&right.name));

        for pair in facets.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(Error::DuplicateFacetName {
                    name: pair[0].name.as_str().to_owned(),
                });
            }
        }

        Ok(Self { facets })
    }

    /// Every facet, ordered by name.
    pub fn facets(&self) -> &[Facet] {
        &self.facets
    }

    /// The facets of one category, ordered by name.
    pub fn facets_in(&self, category: Category) -> impl Iterator<Item = &Facet> {
        self.facets
            .iter()
            .filter(move |facet| facet.category == category)
    }
}
