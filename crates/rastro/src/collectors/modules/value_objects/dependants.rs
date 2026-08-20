//! Which modules depend on a module.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::module_name::ModuleName;

/// The modules that hold a reference to this one.
///
/// A set, so ordering is a property of the type rather than of a `sort` call
/// somebody has to remember. The kernel walks its `source_list` in link order, which
/// carries nothing worth diffing and would churn as unrelated modules load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dependants(BTreeSet<ModuleName>);

impl Dependants {
    pub fn new(dependants: impl IntoIterator<Item = ModuleName>) -> Self {
        Self(dependants.into_iter().collect())
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModuleName> {
        self.0.iter()
    }
}

impl From<&Dependants> for Observation {
    fn from(dependants: &Dependants) -> Self {
        Observation::list(
            dependants
                .iter()
                .map(|dependant| Observation::text(dependant.as_str())),
        )
    }
}
