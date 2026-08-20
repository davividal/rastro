//! Every flag set on one interface.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::interface_flag::InterfaceFlag;

/// The flags of one interface, as the set they are.
///
/// A set, not a list, so ordering is a property of the type rather than of a `sort` call
/// somebody has to remember. The kernel emits them in a fixed bit order today, which is
/// exactly the kind of stability that holds until it does not.
#[derive(Debug, Clone, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct InterfaceFlags(BTreeSet<InterfaceFlag>);

impl InterfaceFlags {
    pub fn new(flags: impl IntoIterator<Item = InterfaceFlag>) -> Self {
        Self(flags.into_iter().collect())
    }

    pub fn iter(&self) -> impl Iterator<Item = &InterfaceFlag> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&InterfaceFlags> for Observation {
    fn from(flags: &InterfaceFlags) -> Self {
        Observation::list(flags.iter().map(|flag| Observation::text(flag.as_str())))
    }
}
