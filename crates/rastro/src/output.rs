//! Where the fingerprint goes.
//!
//! **A file by default, stdout only when asked.** A fingerprint of a real host is megabytes,
//! and a default that puts megabytes on a terminal is a default that punishes the first run.
//! `-o -` restores the pipe for the callers that want one.
//!
//! This does not weaken "stdout carries only the fingerprint": with the document in a file,
//! stdout carries nothing at all.
//!
//! **Written to a sibling and renamed.** A run killed half way must not leave half a document
//! to be diffed, which the README already promises, so the bytes land under a temporary name
//! and the rename publishes them in one step. It is the same thing `rastro-ssh` does with
//! `mktemp` on the remote side, for the same reason.

mod timestamp;

use std::fs;
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use rastro_fingerprint::{Fingerprint, View, json};

pub use timestamp::utc_stamp;

/// How many bytes of the filesystem's 255 a hostname may take.
///
/// Cut rather than refused: a name is a convenience, and no hostname is worth failing a run
/// that already read the whole box.
const HOSTNAME_LIMIT: usize = 64;

/// Where a run was told to put its document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Stdout,
    File(PathBuf),
}

/// What was written, for the caller that wants to say so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub destination: Destination,
    pub bytes: u64,
}

#[derive(Debug)]
pub struct OutputError {
    message: String,
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for OutputError {}

impl Destination {
    /// Where to write, given what the operator asked for and what the run resolved.
    ///
    /// `-` is stdout rather than a file of that name, which is the convention every tool in a
    /// pipeline follows and the one `rastro-ssh` relies on.
    pub fn resolve(requested: Option<&Path>, hostname: Option<&str>, started_at: i64) -> Self {
        match requested {
            Some(path) if path == Path::new("-") => Self::Stdout,
            Some(path) => Self::File(path.to_path_buf()),
            None => Self::File(PathBuf::from(default_file_name(hostname, started_at))),
        }
    }
}

/// The name a run gives its own document.
///
/// `rastro-<host>-<instant>.json`, with no colon in the instant: a name carrying one needs
/// shell quoting, breaks on VFAT and exFAT, and reads as a host separator to `scp`.
///
/// **The hostname is untrusted input.** It comes from `/proc/sys/kernel/hostname`, which is
/// settable, and rastro runs as root — so a hostname of `../../etc/cron.d/evil` would steer
/// the default path out of the working directory. Anything but `[A-Za-z0-9._-]` is dropped and
/// the result is capped; a hostname that survives as nothing is left out altogether, exactly
/// as an unreadable one is.
pub fn default_file_name(hostname: Option<&str>, started_at: i64) -> String {
    let stamp = utc_stamp(started_at);
    let host: String = hostname
        .unwrap_or_default()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .take(HOSTNAME_LIMIT)
        .collect();

    match host.is_empty() {
        true => format!("rastro-{stamp}.json"),
        false => format!("rastro-{host}-{stamp}.json"),
    }
}

/// Writes the document where it was told to.
///
/// Refuses an existing regular file unless `force`, because the whole workflow is a `before`
/// and an `after`, and quietly overwriting the `before` destroys the only record of the state
/// being compared against. That is the one irreversible thing this tool can do to an operator.
pub fn write(
    destination: &Destination,
    fingerprint: &Fingerprint,
    view: View,
    force: bool,
) -> Result<Written, OutputError> {
    let bytes = match destination {
        Destination::Stdout => to_stdout(fingerprint, view)?,
        Destination::File(path) => to_file(path, fingerprint, view, force)?,
    };

    Ok(Written {
        destination: destination.clone(),
        bytes,
    })
}

fn to_stdout(fingerprint: &Fingerprint, view: View) -> Result<u64, OutputError> {
    let stdout = io::stdout();
    let mut writer = BufWriter::with_capacity(256 * 1024, stdout.lock());

    let counted = render(&mut writer, fingerprint, view)
        .and_then(|bytes| writer.flush().map(|()| bytes))
        .map_err(|error| failure("stdout", "could not be written", &error))?;

    Ok(counted)
}

fn to_file(
    path: &Path,
    fingerprint: &Fingerprint,
    view: View,
    force: bool,
) -> Result<u64, OutputError> {
    // Only a regular file is protected: `-o /dev/null` and `-o /dev/stdout` have to keep
    // working, and neither is a fingerprint anybody is about to lose.
    if !force && fs::symlink_metadata(path).is_ok_and(|at| at.file_type().is_file()) {
        return Err(OutputError {
            message: format!(
                "{} already exists, so this run would overwrite a fingerprint that cannot be \
                 taken again; pass --force to replace it or -o to name another path",
                path.display()
            ),
        });
    }

    let staging = staging_path(path);
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .truncate(false)
        // 0600 at creation, not afterwards: a fingerprint names every path on the box, and a
        // chmod after the fact leaves a window where anybody could read it.
        .mode(0o600)
        .open(&staging)
        .map_err(|error| {
            failure(
                &staging.display().to_string(),
                "could not be created",
                &error,
            )
        })?;

    let written = write_and_publish(file, &staging, path, fingerprint, view);
    if written.is_err() {
        // Best effort: the run has already failed, and a leftover partial file is the thing
        // the temporary name existed to prevent.
        let _ = fs::remove_file(&staging);
    }

    written
}

fn write_and_publish(
    file: fs::File,
    staging: &Path,
    path: &Path,
    fingerprint: &Fingerprint,
    view: View,
) -> Result<u64, OutputError> {
    let named = staging.display().to_string();
    let mut writer = BufWriter::with_capacity(256 * 1024, file);

    let bytes = render(&mut writer, fingerprint, view)
        .map_err(|error| failure(&named, "could not be written", &error))?;

    writer
        .flush()
        .map_err(|error| failure(&named, "could not be flushed", &error))?;

    // A fingerprint is often taken immediately before something that reboots the box, so the
    // bytes are on the disk before the rename claims they are.
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| failure(&named, "could not be synced", &error))?;

    // The rename keeps the 0600 the creation set, which a `--force` truncate of an existing
    // 0644 file would not: a mode applies when a file is made, not when it is written.
    fs::rename(staging, path).map_err(|error| {
        failure(
            &path.display().to_string(),
            "could not be replaced with the finished document",
            &error,
        )
    })?;

    Ok(bytes)
}

/// The document, and how many bytes it was.
fn render(writer: &mut impl Write, fingerprint: &Fingerprint, view: View) -> io::Result<u64> {
    let mut counted = Counting { writer, bytes: 0 };
    json::to_canonical_json_writer(fingerprint, view, &mut counted)?;

    Ok(counted.bytes)
}

/// A writer that remembers how much went through it.
///
/// Counted here rather than by asking the filesystem afterwards, so the figure is the same one
/// for stdout, where there is nothing to ask.
struct Counting<'a, W: Write> {
    writer: &'a mut W,
    bytes: u64,
}

impl<W: Write> Write for Counting<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.writer.write(buffer)?;
        self.bytes += written as u64;

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// A sibling of the target, so the rename that publishes it cannot cross a filesystem.
fn staging_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "fingerprint".to_owned());
    let staging = format!(".{name}.{}.partial", std::process::id());

    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(staging),
        _ => PathBuf::from(staging),
    }
}

fn failure(what: &str, happened: &str, error: &io::Error) -> OutputError {
    OutputError {
        message: format!("{what} {happened}: {error}"),
    }
}
