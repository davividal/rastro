//! `/var/lib/nfs/etab`, the export table mountd answers the kernel from.
//!
//! **Read rather than asked for.** `exportfs -v` prints the same table and was measured to
//! recreate `/var/lib/nfs/.etab.lock` on every run, so asking would be a write. Nor is the
//! kernel's `/proc/fs/nfs/exports` the table: the kernel fills it lazily as clients ask, so it
//! holds only the exports somebody has used since they were last flushed.
//!
//! **Effective, not declared.** `exportfs` writes etab from `/etc/exports` and `exports.d` with
//! every default filled in, and rewrites it only when told to, so an edit nobody applied is not
//! here, which is the gap a fingerprint is for. The files themselves are in the walk.

use std::path::{Path, PathBuf};

use rastro_collector::{AbsolutePath, CollectionError};

use crate::collectors::inside_root::read_inside;
use crate::collectors::nfs::model::{ClientExport, ExportTable, SecurityGroup};
use crate::collectors::nfs::value_objects::{ExportClient, ExportOption, SecurityFlavour};

/// Where nfs-utils keeps its state, etab among it.
const STATE_DIRECTORY: &str = "/var/lib/nfs";

/// The table's name inside [`STATE_DIRECTORY`].
const ETAB: &str = "etab";

/// The option that starts a flavour's group, and the separator between flavours in it.
const FLAVOUR_OPTION: &str = "sec=";
const FLAVOUR_SEPARATOR: char = ':';

/// The export table as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Etab {
    state_directory: PathBuf,
}

impl Etab {
    pub fn new() -> Self {
        Self::in_directory(STATE_DIRECTORY)
    }

    /// The same over a state directory the caller names.
    pub fn in_directory(state_directory: impl Into<PathBuf>) -> Self {
        Self {
            state_directory: state_directory.into(),
        }
    }

    pub fn state_directory(&self) -> &Path {
        &self.state_directory
    }

    /// Reads the table, bounded and a regular file only, and translates it.
    pub fn read(&self) -> Result<ExportTable, CollectionError> {
        let table = read_inside(&self.state_directory, Path::new(ETAB)).map_err(|error| {
            CollectionError::new(format!(
                "could not read {}: {error}",
                self.state_directory.join(ETAB).display()
            ))
        })?;

        Self::parse(&table)
    }

    /// Translates the table's text into the model.
    ///
    /// One export per line: the path, a tab, then the client with its options in brackets.
    pub fn parse(table: &str) -> Result<ExportTable, CollectionError> {
        let exports = table
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(parse_line)
            .collect::<Result<Vec<_>, CollectionError>>()?;

        ExportTable::new(exports)
    }
}

impl Default for Etab {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_line(line: &str) -> Result<(AbsolutePath, ExportClient, ClientExport), CollectionError> {
    let unsplittable =
        || CollectionError::new(format!("etab holds a line it cannot split: {line:?}"));

    let (path, entry) = line.split_once('\t').ok_or_else(unsplittable)?;
    let (client, options) = entry
        .strip_suffix(')')
        .and_then(|entry| entry.split_once('('))
        .ok_or_else(unsplittable)?;

    // `exportfs` writes a space in a path as `\040`, and a path rastro cannot spell back must
    // not be reported under a name that is not the export's.
    if path.contains('\\') {
        return Err(CollectionError::new(format!(
            "etab names {path:?}, which carries an escape rastro does not decode"
        )));
    }

    Ok((
        AbsolutePath::new(path, "exported path")?,
        ExportClient::new(client)?,
        parse_options(options)?,
    ))
}

/// The options before the first `sec=` hold for every flavour; each `sec=` starts a group of its own.
fn parse_options(options: &str) -> Result<ClientExport, CollectionError> {
    let mut export = ClientExport {
        options: Vec::new(),
        security: Vec::new(),
    };

    for option in options.split(',') {
        if let Some(flavours) = option.strip_prefix(FLAVOUR_OPTION) {
            export.security.push(SecurityGroup {
                flavours: flavours
                    .split(FLAVOUR_SEPARATOR)
                    .map(SecurityFlavour::new)
                    .collect::<Result<_, _>>()?,
                options: Vec::new(),
            });
            continue;
        }

        let option = ExportOption::new(option)?;
        match export.security.last_mut() {
            Some(group) => group.options.push(option),
            None => export.options.push(option),
        }
    }

    Ok(export)
}
