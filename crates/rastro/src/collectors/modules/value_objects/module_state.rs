//! Where a module is in its life.

use rastro_collector::{CollectionError, Observation};

/// Whether a module is in service or on its way in or out.
///
/// Exactly three, because `m_show` in `kernel/module/procfs.c` prints exactly
/// three: `Unloading` while going, `Loading` while coming, `Live` otherwise. An
/// unrecognised value therefore means the columns were read in the wrong order, not
/// that the kernel grew a state, so it is refused rather than passed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleState {
    Live,
    Loading,
    Unloading,
}

impl ModuleState {
    pub fn parse(value: &str) -> Result<Self, CollectionError> {
        match value {
            "Live" => Ok(Self::Live),
            "Loading" => Ok(Self::Loading),
            "Unloading" => Ok(Self::Unloading),
            other => Err(CollectionError::new(format!(
                "{other:?} is not a module state the kernel reports"
            ))),
        }
    }

    /// The name as the document spells it, which is lower case throughout.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Loading => "loading",
            Self::Unloading => "unloading",
        }
    }
}

impl From<&ModuleState> for Observation {
    fn from(state: &ModuleState) -> Self {
        Observation::text(state.as_str())
    }
}
