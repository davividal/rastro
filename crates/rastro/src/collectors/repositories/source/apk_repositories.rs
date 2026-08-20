//! The `/etc/apk/repositories` interface.
//!
//! One repository per line, and that is the whole grammar. apk divides a repository
//! neither by release nor by component, so a line is a URI, optionally preceded by a
//! tag that pins it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use crate::collectors::repositories::model::{Repository, RepositorySet};
use crate::collectors::repositories::value_objects::{
    Components, Enablement, RepositoryTag, RepositoryUri,
};

/// Where apk lists the repositories it fetches from.
const REPOSITORIES: &str = "/etc/apk/repositories";

/// What marks a tagged repository, as in `@edge https://.../edge/main`.
const TAG_MARKER: char = '@';

/// apk's repository list as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkRepositories {
    path: PathBuf,
}

impl ApkRepositories {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(REPOSITORIES),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Finds apk's repository list, or reports that this host does not use apk.
    pub fn detect() -> Option<Self> {
        let repositories = Self::new();
        repositories.path.is_file().then_some(repositories)
    }

    pub fn read(&self) -> Result<RepositorySet, CollectionError> {
        let text = fs::read_to_string(&self.path).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", self.path.display()))
        })?;

        Self::parse(&text)
    }

    /// Translates the file's text into the model.
    ///
    /// Separate from [`Self::read`] so the whole grammar is exercised from a fixture,
    /// with no apk to install.
    pub fn parse(text: &str) -> Result<RepositorySet, CollectionError> {
        let repositories = text
            .lines()
            .filter_map(parse_line)
            .collect::<Result<Vec<Repository>, CollectionError>>()?;

        Ok(RepositorySet::new(repositories))
    }
}

/// Reads one line, or decides it holds no repository.
///
/// A commented line is recorded as a disabled repository, the same call the one-line
/// apt format makes and for the same reason: commenting a repository out is how it gets
/// switched off. Unlike apt's file, a comment here that is not a repository is still
/// skipped, because a bare word is not a URI and there is nothing to record.
fn parse_line(line: &str) -> Option<Result<Repository, CollectionError>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (enablement, entry) = match trimmed.strip_prefix('#') {
        Some(commented) => (Enablement::Disabled, commented.trim()),
        None => (Enablement::Enabled, trimmed),
    };

    if entry.is_empty() {
        return None;
    }

    Some(parse_entry(entry, enablement))
}

fn parse_entry(entry: &str, enablement: Enablement) -> Result<Repository, CollectionError> {
    let (tag, uri) = match entry.strip_prefix(TAG_MARKER) {
        Some(tagged) => {
            let (tag, uri) = tagged.split_once(char::is_whitespace).ok_or_else(|| {
                CollectionError::new(format!(
                    "an apk repository tag must be followed by a uri, got {entry:?}"
                ))
            })?;
            (Some(RepositoryTag::new(tag)?), uri.trim())
        }
        None => (None, entry),
    };

    Ok(Repository {
        uri: RepositoryUri::new(uri)?,
        enablement,
        // apk has neither concept, so both are absent rather than invented.
        archive_type: None,
        suite: None,
        components: Components::default(),
        tag,
        settings: BTreeMap::new(),
    })
}

impl Default for ApkRepositories {
    fn default() -> Self {
        Self::new()
    }
}
