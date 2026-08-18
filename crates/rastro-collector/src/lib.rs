//! The contract a rastro collector fulfils.
//!
//! **This is the crate to depend on if you are writing a collector.** Implement
//! [`Collector`], answer [`Presence`] honestly, and return an
//! [`Observation`] with its values annotated.
//! Everything you need from the document model is re-exported below, so one
//! dependency is enough.
//!
//! Separate from the identity types it re-exports, which live in
//! `rastro-fingerprint` because every facet records them. That split is what
//! keeps the crate graph acyclic: identity is depended *upon*, this port
//! depends *on* things.

#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod fingerprint_host;
pub mod value_objects;

use thiserror::Error;

/// The vocabulary collectors share, re-exported so one import still covers a
/// collector's needs.
pub use value_objects::{AbsolutePath, NonEmptyText};

/// Everything a collector author needs from the document model, under exactly
/// the names the trait signatures use.
///
/// Renaming them here would mean an author reading `fn category(&self) ->
/// CollectorCategory` could not find `CollectorCategory` in the crate they were
/// told to depend on. Guarded by `tests/one_dependency_is_enough.rs`, which
/// resolves every path it needs through this crate and stops compiling the
/// moment one goes missing.
///
/// That guard is the test's own discipline rather than a boundary cargo draws:
/// `rastro-fingerprint` is a normal dependency here, so the test target could
/// name it directly and still compile. Making it structural would take a
/// fourth workspace member depending on this crate alone, which is more
/// machinery than the regression is worth.
pub use rastro_fingerprint::{
    CollectorCategory, CollectorId, CollectorIdentity, CollectorVersion, Content, FacetName,
    FingerprintError, Observation, Scalar, Sensitivity, View, Volatility,
};

/// Whether a collector's subject is on this host.
///
/// Three-valued on purpose. A collector that cannot tell must be able to say
/// so: reporting `Absent` when the truth is "the check itself failed" writes a
/// confident lie into the fingerprint, and a lie that reads as real state is
/// worse than a recorded failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    Present,
    Absent,
    /// Could not tell, and why. Never silently treated as either of the others.
    Undetermined {
        reason: String,
    },
}

/// A collector ran but could not produce its observation.
///
/// Distinct from absence: the subject is there, reading it did not work.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct CollectionError {
    message: String,
}

/// One source of state, built in or external.
///
/// A collector answers two questions and never mentions facets: assembling a
/// facet from its answers is the use case's job. That keeps every adapter free
/// of the document model it feeds.
pub trait Collector {
    /// The state surface this collector reports on.
    fn name(&self) -> &FacetName;

    /// Which collector, and which version of it, is speaking.
    fn identity(&self) -> &CollectorIdentity;

    fn category(&self) -> CollectorCategory;

    /// Is my subject on this host?
    fn presence(&self) -> Presence;

    /// Its state.
    ///
    /// Only called once presence is established, so it never has to express
    /// absence.
    fn collect(&self) -> Result<Observation, CollectionError>;
}

impl CollectionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
