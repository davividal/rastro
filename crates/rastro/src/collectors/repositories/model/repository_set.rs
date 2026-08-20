//! Every repository one system is configured with.

use rastro_collector::Observation;

use super::repository::Repository;

/// One system's repositories, sorted.
///
/// **A list, because a repository has no unique name to key on**, and sorted, because
/// it has no meaningful order either. That combination is the third case the
/// keyed-or-listed rule has to cover: mounts are listed and left in the host's order
/// because that order carries stacking, packages are keyed because a name is unique,
/// and these are listed because there is no key and sorted because the order carries
/// nothing.
///
/// Sorting is what stops the facet churning. apt reads `sources.list` and then every
/// file in `sources.list.d` in an order that depends on the directory, and a deb822
/// paragraph expands into several entries; none of that is state. Two identical
/// entries in different files are both kept, because apt warns about exactly that and
/// a fingerprint that silently deduplicated them would hide the duplication.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepositorySet(Vec<Repository>);

impl RepositorySet {
    pub fn new(repositories: impl IntoIterator<Item = Repository>) -> Self {
        let mut sorted: Vec<Repository> = repositories.into_iter().collect();
        sorted.sort();

        Self(sorted)
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&RepositorySet> for Observation {
    fn from(set: &RepositorySet) -> Self {
        Observation::list(set.repositories().iter().map(Observation::from))
    }
}
