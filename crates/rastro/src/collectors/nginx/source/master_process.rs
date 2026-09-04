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

/// Where a process's parent is written, and the field that carries it.
const STATUS: &str = "status";
const PARENT_FIELD: &str = "PPid:";

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
                    workers.push(Worker {
                        parent: parent_of(&path),
                        started_at: started_at(&path),
                    });
                }
            }
        }
    }

    // The lowest pid, so that a box somehow running two masters reports the same one twice
    // rather than whichever the directory happened to list first.
    masters.sort_by_key(|master| master.process_id);

    // A master whose executable can be read and matches is the answer. Failing that, one
    // whose executable cannot be read at all still is: it called itself an nginx master, and
    // an unprivileged run cannot do better than believe it.
    let chosen = masters
        .iter()
        .find(|master| {
            executable_of(&master.path).is_some_and(|executable| runs(&executable, binary))
        })
        .or_else(|| {
            masters
                .iter()
                .find(|master| executable_of(&master.path).is_none())
        });

    match chosen {
        Some(master) => describe(master, &workers),
        None => Ok(None),
    }
}

/// One process that named itself an nginx worker.
///
/// Both fields are optional because both are read from a process that may leave between the
/// listing and the read. A worker rastro cannot place is not counted rather than guessed at.
struct Worker {
    parent: Option<i64>,
    started_at: Option<SecondsSinceEpoch>,
}

/// One process that named itself an nginx master.
struct Found {
    process_id: i64,
    path: PathBuf,
    title: String,
}

/// The chosen master, with the workers that belong to *it*.
///
/// **Workers are matched by parent, not by title.** A box running two nginx instances has
/// two sets of workers, and counting all of them against one master would make its worker
/// count wrong and its oldest-worker time — the thing that dates the last reload — belong to
/// somebody else's reload. A binary upgrade produces the same shape for a while, with the
/// old master's workers still running beside the new one's.
fn describe(master: &Found, workers: &[Worker]) -> Result<Option<Master>, CollectionError> {
    let Some(started_at) = started_at(&master.path) else {
        // The master left between the listing and this read, so there is no master to
        // describe. That is the same race the workers below are skipped for, and the same
        // one the `processes` collector treats as a departure rather than a failure.
        return Ok(None);
    };

    let executable = executable_of(&master.path)
        .map(|executable| NonEmptyText::new(executable, "nginx executable"))
        .transpose()?;

    let ours: Vec<SecondsSinceEpoch> = workers
        .iter()
        .filter(|worker| worker.parent == Some(master.process_id))
        .filter_map(|worker| worker.started_at)
        .collect();

    Ok(Some(Master {
        process_id: master.process_id,
        executable,
        started_at,
        configuration_path: flag(&master.title, CONFIGURATION_FLAG)?,
        prefix: flag(&master.title, PREFIX_FLAG)?,
        worker_count: ours.len() as i64,
        workers_started_at: ours.iter().min().copied(),
    }))
}

/// The process that started this one, from the `PPid:` line of its `status`.
fn parent_of(process: &Path) -> Option<i64> {
    let status = fs::read_to_string(process.join(STATUS)).ok()?;

    status
        .lines()
        .find_map(|line| line.strip_prefix(PARENT_FIELD))
        .and_then(|parent| parent.trim().parse().ok())
}

/// What `/proc/<pid>/exe` points at, when the reader is allowed to look.
///
/// Only root, or the process's own owner, may read that link. Everybody else gets
/// `EACCES`, which is why this answers `None` rather than a failure: not knowing which
/// binary a process runs is a fact about the reader, not about the host.
fn executable_of(process: &Path) -> Option<String> {
    fs::read_link(process.join("exe"))
        .ok()
        .map(|executable| executable.to_string_lossy().into_owned())
}

/// Whether an executable is the binary the collector located.
///
/// The ` (deleted)` the kernel appends after a package upgrade is stripped before comparing,
/// which is the difference between reporting an upgraded-but-not-restarted server and
/// reporting no server at all. The marker itself is still recorded, because it is the state.
fn runs(executable: &str, binary: &Path) -> bool {
    let replaced = executable.strip_suffix(DELETED).unwrap_or(executable);

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
///
/// `None` where the directory has gone, which during a reload is ordinary: a worker listed a
/// moment ago is replaced while the scan is still running. Failing the whole facet for that
/// would be the same mistake the `processes` collector carried until this branch fixed it.
fn started_at(process: &Path) -> Option<SecondsSinceEpoch> {
    fs::metadata(process)
        .ok()
        .map(|metadata| SecondsSinceEpoch::new(metadata.mtime()))
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
