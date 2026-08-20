//! The files the localisation settings live in.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::{AbsolutePath, CollectionError};

use crate::collectors::locale::model::Localisation;
use crate::collectors::locale::value_objects::{SettingName, SettingValue};

/// systemd's own file, which a Debian box usually does not have.
const LOCALE_CONF: &str = "/etc/locale.conf";

/// Debian's file, which is the one that exists on the development box.
const DEFAULT_LOCALE: &str = "/etc/default/locale";

/// The virtual console's keymap and font.
const VCONSOLE_CONF: &str = "/etc/vconsole.conf";

/// The localisation files as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleFiles {
    paths: Vec<PathBuf>,
}

impl LocaleFiles {
    pub fn new() -> Self {
        Self {
            paths: [LOCALE_CONF, DEFAULT_LOCALE, VCONSOLE_CONF]
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        }
    }

    /// The same over paths the caller chose.
    pub fn at(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            paths: paths.into_iter().collect(),
        }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Reads every file, reporting one that is absent as absent rather than skipping it.
    pub fn read(&self) -> Result<Localisation, CollectionError> {
        let mut files = Vec::new();

        for path in &self.paths {
            let settings = match fs::read_to_string(path) {
                Ok(contents) => Some(parse(&contents)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(CollectionError::new(format!(
                        "could not read {}: {error}",
                        path.display()
                    )));
                }
            };

            files.push((path_of(path)?, settings));
        }

        Localisation::new(files)
    }
}

/// Reads one file's `KEY=value` lines.
///
/// **Comments and blank lines are skipped, because these files are read as shell fragments
/// and a `#` line is a comment to the shell too.** A line with no `=` is skipped rather
/// than refused: `/etc/default/locale` is sourced by shell scripts, so it may legally hold a
/// bare `export`, and refusing one would lose the whole facet over a line that sets nothing.
///
/// A repeated key is refused. The shell would take the last, so the file is ambiguous about
/// what the box is set to, and quietly resolving it the shell's way would hide a
/// misconfiguration that only shows up when something reads the file differently.
fn parse(contents: &str) -> Result<BTreeMap<SettingName, SettingValue>, CollectionError> {
    let mut settings = BTreeMap::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            continue;
        };

        let name = SettingName::new(name.trim())?;
        if settings
            .insert(name.clone(), SettingValue::new(value))
            .is_some()
        {
            return Err(CollectionError::new(format!(
                "{:?} is set twice, so what the box is localised to depends on which line \
                 a reader takes",
                name.as_str()
            )));
        }
    }

    Ok(settings)
}

fn path_of(path: &Path) -> Result<AbsolutePath, CollectionError> {
    AbsolutePath::new(path.to_string_lossy().into_owned(), "locale file")
}

impl Default for LocaleFiles {
    fn default() -> Self {
        Self::new()
    }
}
