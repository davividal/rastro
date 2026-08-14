//! Who observes: the identity every facet records.
//!
//! Nothing here depends on documents or observations, which is deliberate.
//! A facet records the collector that produced it, so this module is depended
//! upon rather than depending; the contract a collector fulfils lives in the
//! `rastro-collector` crate.

mod identity;

pub use identity::{CollectorId, CollectorIdentity, CollectorVersion};

/// What a collector observes, which decides where its facet lands in the
/// document.
///
/// Both categories share one contract: the same outcomes, the same annotations,
/// the same rendering. The distinction is placement and, later, whether the
/// config may switch the collector off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorCategory {
    /// Describes the run and the box it ran on: which rastro, which config,
    /// which host. Always present, because without it two fingerprints cannot
    /// be told apart or lined up against each other.
    Metadata,
    /// Describes state observed on the host. May be absent, may fail, may be
    /// switched off.
    State,
}
