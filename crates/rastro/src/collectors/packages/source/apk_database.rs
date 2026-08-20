//! The apk installed-package database.
//!
//! Read directly rather than through `apk`, which is the opposite of the choice made for
//! dpkg and needs its reason stated. apk 3 offers no machine-readable output at all: every
//! text form it prints fuses the name and version into one token, so parsing it would mean
//! reimplementing apk's own name-version splitting grammar. The database is one field per
//! line and unambiguous, so it is the more honest source here.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use crate::collectors::packages::model::{Package, PackageSet};
use crate::collectors::packages::value_objects::{Architecture, PackageName, PackageVersion};

/// Where apk records what it has installed.
const INSTALLED: &str = "/lib/apk/db/installed";

/// The fields rastro reads. apk writes a dozen more per package (checksum, size, licence,
/// commit, dependency list) that describe the package rather than the host's state.
const NAME: &str = "P:";
const VERSION: &str = "V:";
const ARCHITECTURE: &str = "A:";

/// apk's database as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkDatabase {
    path: PathBuf,
}

impl ApkDatabase {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(INSTALLED),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Finds apk's database, or reports that this host does not use apk.
    pub fn detect() -> Option<Self> {
        let database = Self::new();
        database.path.is_file().then_some(database)
    }

    /// Reads the database and translates it into the model.
    pub fn read(&self) -> Result<PackageSet, CollectionError> {
        let database = fs::read_to_string(&self.path).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", self.path.display()))
        })?;

        Self::parse(&database)
    }

    /// Translates the database into the model.
    ///
    /// Stanzas are separated by a blank line. A stanza missing a name, a version or an
    /// architecture is refused rather than skipped.
    pub fn parse(database: &str) -> Result<PackageSet, CollectionError> {
        let packages = database
            .split("\n\n")
            .filter(|stanza| !stanza.trim().is_empty())
            .map(Self::parse_stanza)
            .collect::<Result<Vec<(PackageName, Package)>, CollectionError>>()?;

        PackageSet::new(packages)
    }

    fn parse_stanza(stanza: &str) -> Result<(PackageName, Package), CollectionError> {
        // Two names mean two stanzas run together, and `field` takes the first hit, so the second
        // package would simply not be constructed. Every other guard in this collector refuses an
        // input nothing can produce; this is the one whose unguarded outcome is a quietly smaller
        // answer rather than a loud failure.
        let names = stanza.lines().filter(|line| line.starts_with(NAME)).count();
        if names > 1 {
            return Err(CollectionError::new(format!(
                "a stanza in the apk database carries {names} {NAME:?} fields, so two ran \
                 together: {stanza:?}"
            )));
        }

        let name = Self::field(stanza, NAME)?;
        let package = Package {
            version: PackageVersion::new(Self::field(stanza, VERSION)?)?,
            architecture: Architecture::new(Self::field(stanza, ARCHITECTURE)?)?,
            // apk records only what is installed, so there is no desired state to report.
            status: None,
        };

        Ok((PackageName::new(name)?, package))
    }

    fn field<'a>(stanza: &'a str, prefix: &str) -> Result<&'a str, CollectionError> {
        stanza
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "a package in the apk database has no {prefix:?} field: {stanza:?}"
                ))
            })
    }
}

impl Default for ApkDatabase {
    fn default() -> Self {
        Self::new()
    }
}
