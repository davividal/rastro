//! One place a host is configured to fetch packages from.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::repositories::value_objects::{
    ArchiveType, Components, Enablement, RepositoryTag, RepositoryUri, Suite,
};

/// A repository, in rastro's terms rather than any one configuration format's.
///
/// **Two things a repository has, and the split between them is this type's one
/// idea.** The fields that *identify* it are typed, because those are what an operator
/// diffs and what a mistake in parsing would corrupt. Everything else a format lets
/// you say about a repository goes verbatim into [`Self::settings`].
///
/// The reason is that the long tail is genuinely long and genuinely open. A deb822
/// paragraph may carry `Signed-By`, `Architectures`, `Languages`, `Targets`, `PDiffs`,
/// `By-Hash`, `Check-Valid-Until`, `Valid-Until-Min`, `Date-Max-Future`,
/// `InRelease-Path`, `Snapshot` and `Trusted`, and a one-line entry may carry any of
/// them inside its brackets. Modelling each would be a dozen value objects that mostly
/// never appear; refusing the ones rastro does not know would fail the facet on a
/// perfectly ordinary box the first time apt gained a field. Recording them as written
/// keeps the facet complete, which is the property this project will not trade, and
/// leaves them diffable, which is what they are for.
///
/// **`signed-by` deliberately sits in that map rather than in a typed field**, even
/// though it is the security-relevant one. It is configuration of a repository, not
/// part of its identity: the same repository signed by a new keyring is still that
/// repository, and the change shows up as a changed setting rather than as one
/// repository replaced by another.
///
/// **The optional fields are optional because systems differ**, which is the same
/// shape the packages collector uses when apk cannot report dpkg's desired state. apt
/// scopes a repository by archive type and suite; apk does not have either, and apk
/// tags a repository, which apt does not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Repository {
    pub uri: RepositoryUri,
    pub enablement: Enablement,
    /// Absent for systems that do not separate built packages from their sources.
    pub archive_type: Option<ArchiveType>,
    /// Absent for systems that do not scope a repository by release.
    pub suite: Option<Suite>,
    /// Empty for a flat apt repository and for every apk repository.
    pub components: Components,
    /// Present only where a system lets a repository be pinned by label.
    pub tag: Option<RepositoryTag>,
    /// Every other option the entry carried, as the format wrote it.
    pub settings: BTreeMap<String, String>,
}

impl From<&Repository> for Observation {
    fn from(repository: &Repository) -> Self {
        Observation::object([
            (
                "archive_type",
                repository
                    .archive_type
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("components", Observation::from(&repository.components)),
            ("enablement", Observation::from(&repository.enablement)),
            (
                "settings",
                Observation::object(
                    repository
                        .settings
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::text(value.clone()))),
                ),
            ),
            (
                "suite",
                repository
                    .suite
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "tag",
                repository
                    .tag
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("uri", Observation::from(&repository.uri)),
        ])
    }
}
