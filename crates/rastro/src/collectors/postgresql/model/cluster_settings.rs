//! Every setting a cluster is running with.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::Setting;

/// The effective configuration of one cluster.
///
/// Holds two invariants the individual settings cannot: each name appears once, and there
/// is at least one setting. Both refusals exist because the alternative is a document that
/// reads as state: a name appearing twice would silently keep whichever value happened to
/// be rendered last, and an empty set would report a configured cluster as having no
/// configuration at all. A real cluster has hundreds, so nothing means the read failed.
///
/// Sorted by name here rather than left to the server, because list order is part of the
/// output contract and `ORDER BY` in a query is a promise made somewhere this type cannot
/// see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterSettings {
    settings: Vec<Setting>,
}

impl ClusterSettings {
    pub fn new(mut settings: Vec<Setting>) -> Result<Self, CollectionError> {
        if settings.is_empty() {
            return Err(CollectionError::new(
                "the server reported no settings at all, which no running cluster does",
            ));
        }

        settings.sort_by(|left, right| left.name.cmp(&right.name));

        if let Some(repeated) = settings
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.as_str())
        {
            return Err(CollectionError::new(format!(
                "the server reported the setting {repeated:?} twice, so which value it is \
                 running with cannot be told"
            )));
        }

        Ok(Self { settings })
    }

    pub fn settings(&self) -> &[Setting] {
        &self.settings
    }
}

impl From<&ClusterSettings> for Observation {
    fn from(settings: &ClusterSettings) -> Self {
        Observation::object(
            settings
                .settings()
                .iter()
                .map(|setting| (setting.name.as_str(), Observation::from(setting))),
        )
    }
}
