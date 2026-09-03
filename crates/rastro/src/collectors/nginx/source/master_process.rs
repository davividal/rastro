//! The `/proc` interface: finding the nginx that is running.
//!
//! nginx rewrites its own argument vector into a process title, so `/proc/<pid>/cmdline`
//! reads `nginx: master process /usr/sbin/nginx -c /etc/nginx/nginx.conf`. That title is
//! what tells a master from a worker, and it is also the only place the command line that
//! started the service can be read back from the process itself.
//!
//! **A start time from a directory's mtime**, which is measured rather than assumed:
//! `/proc/<pid>` carries the moment the process began, in whole seconds, and a reload leaves
//! the master's untouched while every worker gets a new one. Reading it this way needs no
//! clock-tick arithmetic, no `btime`, and no `sysconf` — which matters here, because this
//! workspace forbids unsafe code and `sysconf` is a call rather than a constant.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::nginx::model::Master;
use crate::collectors::nginx::value_objects::SecondsSinceEpoch;

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

/// What nginx calls its master process, and what it calls the rest.
const MASTER_TITLE: &str = "nginx: master process";
const WORKER_TITLE: &str = "nginx: worker process";

/// The flags whose values decide which configuration the running server read.
const CONFIGURATION_FLAG: &str = "-c";
const PREFIX_FLAG: &str = "-p";

/// What the kernel appends to a link whose target has been replaced.
const DELETED: &str = " (deleted)";

/// The running master, if this box has one.
pub fn find(binary: &Path) -> Result<Option<Master>, CollectionError> {
    find_in(Path::new(PROC), binary)
}

/// The same over a process table the caller names, so this can be exercised on a fixture.
pub fn find_in(proc: &Path, binary: &Path) -> Result<Option<Master>, CollectionError> {
    let mut masters = Vec::new();
    let mut workers = Vec::new();

    let Ok(entries) = fs::read_dir(proc) else {
        return Ok(None);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(process_id) = process_id_of(&path) else {
            continue;
        };

        let Ok(title) = title_of(&path) else {
            continue;
        };

        match title.starts_with(MASTER_TITLE) {
            true => masters.push(Found {
                process_id,
                path,
                title,
            }),
            false => {
                if title.starts_with(WORKER_TITLE) {
                    workers.push(started_at(&path)?);
                }
            }
        }
    }

    // The lowest pid, so that a box somehow running two masters reports the same one twice
    // rather than whichever the directory happened to list first.
    masters.sort_by_key(|master| master.process_id);

    masters
        .into_iter()
        .find(|master| runs(&master.path, binary))
        .map(|master| describe(&master, &workers))
        .transpose()
}

/// One process that named itself an nginx master.
struct Found {
    process_id: i64,
    path: PathBuf,
    title: String,
}

fn describe(master: &Found, workers: &[SecondsSinceEpoch]) -> Result<Master, CollectionError> {
    let executable = fs::read_link(master.path.join("exe")).map_err(|error| {
        CollectionError::new(format!(
            "the running nginx is process {} and its executable could not be read: {error}",
            master.process_id
        ))
    })?;

    Ok(Master {
        process_id: master.process_id,
        executable: NonEmptyText::new(executable.to_string_lossy(), "nginx executable")?,
        started_at: started_at(&master.path)?,
        configuration_path: flag(&master.title, CONFIGURATION_FLAG)?,
        prefix: flag(&master.title, PREFIX_FLAG)?,
        worker_count: workers.len() as i64,
        workers_started_at: workers.iter().min().copied(),
    })
}

/// Whether this process is running the binary the collector located.
///
/// The ` (deleted)` the kernel appends after a package upgrade is stripped before comparing,
/// which is the difference between reporting an upgraded-but-not-restarted server and
/// reporting no server at all. The marker itself is still recorded, because it is the state.
fn runs(process: &Path, binary: &Path) -> bool {
    let Ok(executable) = fs::read_link(process.join("exe")) else {
        return false;
    };

    let executable = executable.to_string_lossy();
    let replaced = executable.strip_suffix(DELETED).unwrap_or(&executable);

    Path::new(replaced) == binary
}

fn process_id_of(path: &Path) -> Option<i64> {
    path.file_name()?.to_str()?.parse().ok()
}

/// The process title, with the NULs the kernel separates arguments with turned into spaces.
fn title_of(process: &Path) -> Result<String, std::io::Error> {
    let raw = fs::read(process.join("cmdline"))?;

    Ok(String::from_utf8_lossy(&raw).replace('\0', " "))
}

/// When the process began, from the mtime of its own directory.
fn started_at(process: &Path) -> Result<SecondsSinceEpoch, CollectionError> {
    let metadata = fs::metadata(process).map_err(|error| {
        CollectionError::new(format!(
            "{} could not be read, so nginx's start time is unknown: {error}",
            process.display()
        ))
    })?;

    Ok(SecondsSinceEpoch::new(metadata.mtime()))
}

/// The value of a flag in the process title.
///
/// **The title has lost its quoting**, exactly as `systemctl show` has lost a unit's, so this
/// splits on whitespace and takes the token after the flag. nginx also accepts a value glued
/// to its flag (`-c/etc/nginx/other.conf`), which is why the flag is not simply compared for
/// equality. A path holding a space cannot be recovered from either form, and would come
/// back cut at the space.
fn flag(title: &str, name: &str) -> Result<Option<NonEmptyText>, CollectionError> {
    let mut tokens = title.split_whitespace();

    while let Some(token) = tokens.next() {
        let Some(rest) = token.strip_prefix(name) else {
            continue;
        };

        let value = match rest.is_empty() {
            true => tokens.next(),
            false => Some(rest),
        };

        return value
            .map(|value| NonEmptyText::new(value, "nginx command line value"))
            .transpose();
    }

    Ok(None)
}
