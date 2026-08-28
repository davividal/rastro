//! Every extension a cluster could install, keyed by name.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::AvailableExtension;

/// The `pg_available_extensions` of one cluster, keyed by name.
///
/// **Empty is allowed rather than refused.** A stock cluster always has at least `plpgsql`
/// available, but a build could carry no extension control files at all, and that is a real
/// state rather than a failed read. The same name twice is refused: `pg_available_extensions`
/// reads one control file per name, so a repeat means two reads were spliced and which
/// version is available could not be told.
///
/// Rendered as an object keyed by name, whose order the format owns; the type only holds the
/// uniqueness the individual entries cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterAvailableExtensions {
    extensions: Vec<AvailableExtension>,
}

impl ClusterAvailableExtensions {
    pub fn new(mut extensions: Vec<AvailableExtension>) -> Result<Self, CollectionError> {
        extensions.sort();

        if let Some(pair) = extensions
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
        {
            return Err(CollectionError::new(format!(
                "the server reported the extension {:?} available twice, so which version is \
                 offered cannot be told",
                pair[0].name.as_str()
            )));
        }

        Ok(Self { extensions })
    }

    pub fn extensions(&self) -> &[AvailableExtension] {
        &self.extensions
    }
}

impl From<&ClusterAvailableExtensions> for Observation {
    fn from(extensions: &ClusterAvailableExtensions) -> Self {
        Observation::object(
            extensions
                .extensions()
                .iter()
                .map(|extension| (extension.name.as_str(), Observation::from(extension))),
        )
    }
}
