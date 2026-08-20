//! The places cron keeps its jobs.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::crontab::{self, OwnerColumn};
use crate::collectors::cron::model::{CronState, CronTable};
use crate::collectors::cron::value_objects::ScriptName;

/// The system crontab, the one file with an account column that is not a drop-in.
const SYSTEM_CRONTAB: &str = "/etc/crontab";

/// Where packages drop system crontabs.
const DROP_IN_DIRECTORY: &str = "/etc/cron.d";

/// Where a user's own crontab lives once `crontab -e` has written it.
const SPOOL_DIRECTORY: &str = "/var/spool/cron/crontabs";

/// The run-parts directories, whose scripts are scheduled by `/etc/crontab` rather than by
/// anything inside them.
const RUN_PARTS_DIRECTORIES: [&str; 4] = [
    "/etc/cron.hourly",
    "/etc/cron.daily",
    "/etc/cron.weekly",
    "/etc/cron.monthly",
];

/// Everything cron reads, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronFiles {
    system_crontab: PathBuf,
    drop_ins: PathBuf,
    spool: PathBuf,
    run_parts: Vec<PathBuf>,
}

impl CronFiles {
    pub fn new() -> Self {
        Self {
            system_crontab: PathBuf::from(SYSTEM_CRONTAB),
            drop_ins: PathBuf::from(DROP_IN_DIRECTORY),
            spool: PathBuf::from(SPOOL_DIRECTORY),
            run_parts: RUN_PARTS_DIRECTORIES
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        }
    }

    /// The same rooted under a directory the caller chose, which is what makes the whole walk
    /// testable without an `/etc`.
    pub fn under(root: &Path) -> Self {
        let joined = |absolute: &str| root.join(absolute.trim_start_matches('/'));

        Self {
            system_crontab: joined(SYSTEM_CRONTAB),
            drop_ins: joined(DROP_IN_DIRECTORY),
            spool: joined(SPOOL_DIRECTORY),
            run_parts: RUN_PARTS_DIRECTORIES.into_iter().map(joined).collect(),
        }
    }

    /// Reads every source cron would.
    pub fn read(&self) -> Result<CronState, CollectionError> {
        let mut sources = Vec::new();

        sources.push((
            display(&self.system_crontab),
            self.read_crontab(&self.system_crontab, OwnerColumn::Present)?,
        ));

        for path in self.entries(&self.drop_ins)? {
            let table = self.read_crontab(&path, OwnerColumn::Present)?;
            sources.push((display(&path), table));
        }

        // Keyed by account rather than by path: a user crontab's filename *is* the account,
        // and the spool path is an implementation detail of cron's storage.
        for path in self.entries(&self.spool)? {
            let account = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| display(&path));
            let table = self.read_crontab(&path, OwnerColumn::Absent)?;
            sources.push((account, table));
        }

        for directory in &self.run_parts {
            sources.push((display(directory), self.read_scripts(directory)?));
        }

        CronState::new(sources)
    }

    fn read_crontab(
        &self,
        path: &Path,
        owner_column: OwnerColumn,
    ) -> Result<Option<CronTable>, CollectionError> {
        let Some(contents) = read_optional(path)? else {
            return Ok(None);
        };

        crontab::parse(&contents, owner_column)
            .map(Some)
            .map_err(|error| CollectionError::new(format!("in {}: {error}", path.display())))
    }

    /// The scripts in one run-parts directory.
    ///
    /// A directory that is not there reports as absent rather than as empty, which are
    /// different facts: no `/etc/cron.weekly` at all is not the same as one with nothing in it.
    fn read_scripts(&self, directory: &Path) -> Result<Option<CronTable>, CollectionError> {
        if !directory.is_dir() {
            return Ok(None);
        }

        let scripts = self
            .entries(directory)?
            .into_iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .map(ScriptName::new)
            .collect::<Result<Vec<ScriptName>, CollectionError>>()?;

        Ok(Some(CronTable::scripts(scripts)))
    }

    /// The entries of a directory cron would act on, sorted.
    ///
    /// **A name containing a `.` is skipped, because cron and `run-parts` skip it too.** Both
    /// require a name of letters, digits, underscores and hyphens, which is why every Debian
    /// cron directory can carry a `.placeholder` file that nothing ever runs. Reporting one as
    /// a scheduled job would put a job in the fingerprint that the box does not run, and the
    /// same rule quietly covers the `.dpkg-old` and `.bak` files that accumulate beside real
    /// ones.
    fn entries(&self, directory: &Path) -> Result<Vec<PathBuf>, CollectionError> {
        if !directory.is_dir() {
            return Ok(Vec::new());
        }

        let mut found = Vec::new();
        for entry in fs::read_dir(directory).map_err(|error| {
            CollectionError::new(format!("could not list {}: {error}", directory.display()))
        })? {
            let entry = entry.map_err(|error| {
                CollectionError::new(format!(
                    "could not list an entry of {}: {error}",
                    directory.display()
                ))
            })?;

            let name = entry.file_name().to_string_lossy().into_owned();
            if name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
                && !name.is_empty()
            {
                found.push(entry.path());
            }
        }
        found.sort();

        Ok(found)
    }
}

fn read_optional(path: &Path) -> Result<Option<String>, CollectionError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CollectionError::new(format!(
            "could not read {}: {error}",
            path.display()
        ))),
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Default for CronFiles {
    fn default() -> Self {
        Self::new()
    }
}
