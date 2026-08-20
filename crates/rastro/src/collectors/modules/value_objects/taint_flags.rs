//! Every taint a module carries.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::taint_flag::TaintFlag;

/// The taints attributed to one module.
///
/// Empty for the overwhelming majority of modules, and worth reading closely when it
/// is not: an out-of-tree or unsigned module appearing on a box that had neither is
/// the kind of change this tool exists to surface.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaintFlags(BTreeSet<TaintFlag>);

impl TaintFlags {
    pub fn new(flags: impl IntoIterator<Item = TaintFlag>) -> Self {
        Self(flags.into_iter().collect())
    }

    pub fn iter(&self) -> impl Iterator<Item = &TaintFlag> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&TaintFlags> for Observation {
    fn from(flags: &TaintFlags) -> Self {
        Observation::list(flags.iter().map(|flag| Observation::text(flag.to_name())))
    }
}
