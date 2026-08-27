//! What is installed inside one database.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

use crate::collectors::postgresql::value_objects::ExtensionName;

/// One extension installed in a database.
///
/// The version and the schema are both recorded because both change without the other: an
/// `ALTER EXTENSION … UPDATE` moves the version, and the schema decides whether the
/// extension's functions are on anybody's `search_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extension {
    pub name: ExtensionName,
    pub version: NonEmptyText,
    pub schema: NonEmptyText,
}

/// Every extension in one database, ordered by name.
///
/// **Empty is an ordinary answer**, unlike a cluster with no databases: `plpgsql` arrives
/// with a new database and can be dropped, so a database with nothing installed is state.
/// Only a repeated name is refused, because an extension is installed once per database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseExtensions {
    extensions: Vec<Extension>,
}

impl DatabaseExtensions {
    pub fn new(mut extensions: Vec<Extension>) -> Result<Self, CollectionError> {
        extensions.sort_by(|left, right| left.name.cmp(&right.name));

        if let Some(repeated) = extensions
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.as_str())
        {
            return Err(CollectionError::new(format!(
                "the server reported the extension {repeated:?} twice in one database, so which \
                 version is installed cannot be told"
            )));
        }

        Ok(Self { extensions })
    }

    pub fn extensions(&self) -> &[Extension] {
        &self.extensions
    }
}

impl From<&DatabaseExtensions> for Observation {
    fn from(extensions: &DatabaseExtensions) -> Self {
        Observation::object(extensions.extensions().iter().map(|extension| {
            (
                extension.name.as_str(),
                Observation::object([
                    ("version", Observation::text(extension.version.as_str())),
                    ("schema", Observation::text(extension.schema.as_str())),
                ]),
            )
        }))
    }
}
