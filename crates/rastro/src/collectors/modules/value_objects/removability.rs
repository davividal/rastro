//! Whether a module can ever be taken out again.

use rastro_collector::Observation;

/// Whether a module can be unloaded.
///
/// A module with an init function and no exit function can never be removed, and the
/// kernel says so by listing `[permanent]` among its dependants. Recorded as its own
/// value rather than left in the dependant list, where it would read as a module
/// called `[permanent]`.
///
/// Named rather than a `bool` so the document says `"permanent"` instead of
/// `"removability": true`, which would leave the reader guessing which way round it
/// runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Removability {
    Removable,
    Permanent,
}

impl Removability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Removable => "removable",
            Self::Permanent => "permanent",
        }
    }
}

impl From<&Removability> for Observation {
    fn from(removability: &Removability) -> Self {
        Observation::text(removability.as_str())
    }
}
