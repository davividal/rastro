//! Every component a repository entry enables.

use std::collections::BTreeSet;

use rastro_collector::Observation;

use super::component::Component;

/// The components of one repository, as the set it is.
///
/// A set, not a list, so ordering is a property of the type rather than of a `sort`
/// call somebody has to remember. `main contrib` and `contrib main` enable the same
/// thing, and a diff should say nothing when somebody reorders them.
///
/// **Empty is legal and means something.** A flat repository has no components at all,
/// and so does every apk repository, because apk does not divide a repository that
/// way. So an empty set here is not a parse that came up short.
#[derive(Debug, Clone, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Components(BTreeSet<Component>);

impl Components {
    pub fn new(components: impl IntoIterator<Item = Component>) -> Self {
        Self(components.into_iter().collect())
    }

    /// The components in order, which is the sorted order of their names.
    pub fn iter(&self) -> impl Iterator<Item = &Component> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&Components> for Observation {
    fn from(components: &Components) -> Self {
        Observation::list(
            components
                .iter()
                .map(|component| Observation::text(component.as_str())),
        )
    }
}
