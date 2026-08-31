//! Where the fingerprint goes.
//!
//! **A file by default, stdout only when asked.** A fingerprint of a real host is megabytes,
//! and a default that puts megabytes on a terminal is a default that punishes the first run.
//! `-o -` restores the pipe for the callers that want one.
//!
//! This does not weaken "stdout carries only the fingerprint": with the document in a file,
//! stdout carries nothing at all.
//!
//! **Written to a sibling and published in one step.** A run killed half way must not leave
//! half a document to be diffed, which the README already promises, so the bytes land under a
//! temporary name and appear at their own only once complete. It is the same thing
//! `rastro-ssh` does with `mktemp` on the remote side, for the same reason.
//!
//! Two things that shape how that step happens, both because rastro runs as root: a
//! destination that is not a regular file is written *through* rather than replaced, and
//! without `--force` the appearance of the document is a `link` so that the kernel refuses an
//! overwrite rather than a check taken seconds earlier.

/// The path the operator named, as the walk will meet it.
///
/// Exported because the composition root resolves it once and hands the same value to the walk
/// that omits it and to the `invocation` facet that declares it. Resolved rather than merely
/// made absolute: `std::path::absolute` is lexical, so `-o linked/fp.json` through a symlinked
/// directory keeps the symlink, while the walk never follows one and meets the file under its
/// real directory. The two spellings would not match and the previous document would land back
/// in the next run.
pub fn as_walked(path: &Path) -> PathBuf {
    let Some(name) = path.file_name() else {
        return path.to_path_buf();
    };

    match path.parent().map(Path::canonicalize) {
        Some(Ok(directory)) => directory.join(name),
        // Nothing to resolve against yet, so lexical is the best available answer.
        _ => std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
    }
}

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
    // **A destination that is not a regular file is written through, never published over.**
    // `-o /dev/null` and `-o /dev/stdout` have to keep working, and rastro runs as root — so
    // renaming a staged file over either would replace the device or the symlink itself and
    // leave the box worse than this found it. There is nothing to make atomic about a stream:
    // no reader is going to diff it later.
    if let Ok(existing) = fs::symlink_metadata(path) {
        if !existing.file_type().is_file() {
            return straight_into(path, fingerprint, view);
        }

        if !force {
            return Err(already_there(path));
        }
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
        // Named for the destination, not for the temporary file. An operator who typed
        // `-o closed/before.json` is told about that path; the staging name is rastro's
        // business and mentioning it would send them looking for a file they never chose.
        .map_err(|error| failure(&path.display().to_string(), "could not be written", &error))?;

    let written = write_and_publish(file, &staging, path, fingerprint, view, force);
    if written.is_err() {
        // Best effort: the run has already failed, and a leftover partial file is the thing
        // the temporary name existed to prevent.
        let _ = fs::remove_file(&staging);
    }

    written
}

/// Writes to a destination that already exists and is not a regular file.
///
/// Opened and truncated in place, because the point of naming a device, a FIFO or
/// `/dev/stdout` is to write *through* it. Not synced either: there is nothing durable behind
/// it to flush.
fn straight_into(path: &Path, fingerprint: &Fingerprint, view: View) -> Result<u64, OutputError> {
    let named = path.display().to_string();
    let file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| failure(&named, "could not be opened", &error))?;

    let mut writer = BufWriter::with_capacity(256 * 1024, file);
    let bytes = render(&mut writer, fingerprint, view)
        .map_err(|error| failure(&named, "could not be written", &error))?;
    writer
        .flush()
        .map_err(|error| failure(&named, "could not be flushed", &error))?;

    Ok(bytes)
}

fn already_there(path: &Path) -> OutputError {
    OutputError {
        message: format!(
            "{} already exists, so this run would overwrite a fingerprint that cannot be \
             taken again; pass --force to replace it or -o to name another path",
            path.display()
        ),
    }
}

fn write_and_publish(
    file: fs::File,
    staging: &Path,
    path: &Path,
    fingerprint: &Fingerprint,
    view: View,
    force: bool,
) -> Result<u64, OutputError> {
    // The destination throughout, for the reason above: the staging file is an implementation
    // detail of getting there, and a disk that filled up is a fact about where this was going.
    let named = path.display().to_string();
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

    publish(staging, path, force)?;

    Ok(bytes)
}

/// Moves the finished document into place.
///
/// **Without `--force` the refusal is the kernel's, not an earlier check's.** `link` fails with
/// `EEXIST` if the target is there, so a file that appeared while this run was rendering — a
/// window as long as the document is big — cannot be silently replaced. A `rename` would take
/// it, which would break the one promise this module makes about not destroying a `before`.
///
/// Either way the 0600 from creation is kept, which a `--force` truncate of an existing 0644
/// file would not have been: a mode applies when a file is made, not when it is written.
fn publish(staging: &Path, path: &Path, force: bool) -> Result<(), OutputError> {
    if force {
        return fs::rename(staging, path).map_err(|error| {
            failure(
                &path.display().to_string(),
                "could not be replaced with the finished document",
                &error,
            )
        });
    }

    fs::hard_link(staging, path).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => already_there(path),
        _ => failure(
            &path.display().to_string(),
            "could not be created from the finished document",
            &error,
        ),
    })?;

    // The document is at its name now; the staging link is what is left to drop. Named for
    // the staging file here, uniquely, because that *is* the file the operator would have to
    // go and remove.
    fs::remove_file(staging).map_err(|error| {
        failure(
            &staging.display().to_string(),
            "was published but could not be unlinked",
            &error,
        )
    })
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

    /// Required by the trait and never called: the `BufWriter` wrapping this is what gets
    /// flushed, so the flush never reaches down here. Delegating anyway, because a writer that
    /// silently dropped a flush would be a trap for the next caller.
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
