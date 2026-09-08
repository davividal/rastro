//! `docker volume inspect`: docker's spelling of one volume.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use crate::collectors::containers::model::DockerVolume;
use crate::collectors::containers::value_objects::{EngineInstant, LabelName, VolumeName};

/// A volume as docker describes it, kept apart from rastro's meaning.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerVolumeDocument {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Driver")]
    driver: String,
    #[serde(rename = "Mountpoint")]
    mountpoint: String,
    /// Second resolution, unlike a container's stamp, which is nanoseconds.
    #[serde(rename = "CreatedAt")]
    created: String,
    #[serde(rename = "Scope")]
    scope: String,
    /// Null on a volume created with none, for both of these.
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
    #[serde(rename = "Options", default)]
    options: Option<BTreeMap<String, String>>,
}

impl DockerVolumeDocument {
    /// Translates docker's document into rastro's model, keyed by the name it sits under.
    pub fn to_volume(&self) -> Result<(VolumeName, DockerVolume), CollectionError> {
        let mut labels = BTreeMap::new();
        for (name, value) in self.labels.iter().flatten() {
            labels.insert(LabelName::new(name.clone())?, value.clone());
        }

        let mut options = BTreeMap::new();
        for (name, value) in self.options.iter().flatten() {
            options.insert(
                NonEmptyText::new(name.clone(), "volume driver option")?,
                value.clone(),
            );
        }

        let volume = DockerVolume {
            driver: NonEmptyText::new(self.driver.clone(), "volume driver")?,
            mountpoint: AbsolutePath::new(self.mountpoint.clone(), "volume mountpoint")?,
            created: EngineInstant::new(self.created.clone())?,
            scope: NonEmptyText::new(self.scope.clone(), "volume scope")?,
            labels,
            options,
        };

        Ok((VolumeName::new(self.name.clone())?, volume))
    }
}
