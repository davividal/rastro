//! The `EnvironmentFile=` format, as systemd reads it.
//!
//! **This is the nginx exception taken a second time, and it should read as one.** The rule
//! is to prefer effective state over parsing a config, and `systemctl show -p Environment`
//! is that effective state for what a unit *declares*. It deliberately does not cover these
//! files: systemd opens them when it execs the process, not when it loads the unit, so a
//! service configured entirely through one declares an empty environment. There is no
//! non-mutating way to ask systemd what they contribute — anything that resolves them starts
//! the unit — so the format is read directly.
//!
//! What that costs is the risk of disagreeing with systemd's own parser. Two things bound it.
//! Every rule below was measured against systemd 257 rather than read out of
//! `systemd.exec(5)`, by pointing a unit with `ExecStart=/usr/bin/env` at a probe file. And
//! values are marked sensitive, so a value this parser gets subtly wrong still digests
//! deterministically and still diffs; only `--raw` would show the divergence. Names are not
//! withheld, which is why the line-level rules matter more than the escapes and are where the
//! tests concentrate.
//!
//! # Where this format is not the `Environment=` line of a unit file
//!
//! Two traps, both measured, both of which a shared escape table would get wrong:
//!
//! - `"a\nb"` here is a backslash and an `n`. In a unit file it is a line feed.
//! - A backslash *outside* quotes is an escape and vanishes (`a\b` is `ab`), while inside
//!   quotes it is kept unless it precedes `"`, `\` or the end of the line.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use rastro_collector::{AbsolutePath, EnvironmentVariableName};

use crate::collectors::file_glob;
use crate::collectors::systemd::EnvironmentFile;
use crate::collectors::units::model::{EnvironmentReading, EnvironmentSource};

/// What one environment file sets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentFileContents {
    /// Keyed by name, so a diff has one order and a repeat has one answer.
    pub variables: BTreeMap<EnvironmentVariableName, String>,
    /// How many lines systemd would set nothing from, this parser agreeing.
    ///
    /// **Counted rather than dropped, because the commonest one is a mistake.**
    /// `export FOO=bar` is what an operator writes out of shell habit; the name would
    /// contain a space, so systemd sets nothing and the file still looks right. A count
    /// makes that visible without this parser guessing at what was meant.
    pub ignored_lines: usize,
}

/// Two orders of magnitude above the largest real environment file.
///
/// These are read by systemd itself at every service start, so one large enough to matter is
/// already a misconfiguration. The bound exists so that no value of "the operator pointed a
/// unit at something enormous" stops a fingerprint from completing.
const LARGEST_ENVIRONMENT_FILE: u64 = 1024 * 1024;

/// Opens one declared environment file, recording what happened either way.
///
/// **Never fails the facet.** A file that is not there is state, and one that will not open
/// is a failure of that file rather than of the box's whole unit table — the same call the
/// nginx collector makes when an included configuration will not read. Losing the enablement
/// state of every unit because one service's environment file is root-only would be the
/// worse trade by a distance, and rastro is run unprivileged often enough for that to be
/// routine rather than hypothetical.
///
/// `NotFound` is the only errno treated as absence. Anything else, permission denied most of
/// all, is recorded with its message: reporting `absent` for a file rastro was merely not
/// allowed to read would be a confident lie about the box, which is the distinction the
/// three-valued [`Presence`](rastro_collector::Presence) exists for one level up.
///
/// **The path comes from unit configuration, so it is not assumed to be a file.** A unit may
/// name a FIFO, a character device such as `/dev/zero`, or something far larger than any
/// environment file. Each would hang or exhaust a plain `read_to_string`, and one such
/// declaration anywhere on the box would stop the whole run — so the type is checked before
/// anything is opened, and the read is bounded.
/// **A declaration may be a wildcard**, and `systemctl show` reports it as written rather
/// than expanded — measured, `EnvironmentFile=/etc/conf.d/*.env` comes back with the `*`
/// intact. Reading that string directly finds nothing and would report the whole set as
/// absent, which silently loses exactly the variables this facet exists to name. So a
/// pattern is expanded here, in byte order, which is the order systemd applies them in: a
/// variable set in two matched files takes the value from the later one.
///
/// A pattern that matches nothing still produces one entry, with no `path`. That is not
/// tidiness: a *required* wildcard matching nothing stops the unit from starting, measured
/// as `Result=resources`, so it is a finding rather than an empty set to omit.
pub fn read(declared: EnvironmentFile) -> Vec<EnvironmentSource> {
    let pattern = Path::new(declared.path.as_str());

    if !file_glob::is_pattern(pattern) {
        let reading = reading_of(declared.path.as_str());
        let resolved = Some(declared.path.clone());

        return vec![EnvironmentSource {
            declared,
            resolved,
            reading,
        }];
    }

    match file_glob::matching(pattern) {
        Ok(matched) if matched.is_empty() => vec![EnvironmentSource {
            declared,
            resolved: None,
            reading: EnvironmentReading::Absent,
        }],
        Ok(matched) => matched
            .iter()
            .map(|path| {
                let path = path.to_string_lossy().into_owned();
                let reading = reading_of(&path);

                EnvironmentSource {
                    declared: declared.clone(),
                    resolved: AbsolutePath::new(path, "unit environment file").ok(),
                    reading,
                }
            })
            .collect(),
        // A pattern rastro will not resolve, such as a bracket expression. Recorded against
        // the declaration rather than guessed at, for the reason `file_glob` gives.
        Err(refusal) => vec![EnvironmentSource {
            declared,
            resolved: None,
            reading: EnvironmentReading::Unreadable(refusal.to_string()),
        }],
    }
}

/// What one concrete file yielded.
fn reading_of(path: &str) -> EnvironmentReading {
    match contents_of(path) {
        Ok(contents) => EnvironmentReading::Read {
            variables: contents.variables,
            ignored_lines: contents.ignored_lines,
        },
        Err(reading) => reading,
    }
}

/// What the file holds, or the reading that stands in its place.
fn contents_of(path: &str) -> Result<EnvironmentFileContents, EnvironmentReading> {
    // Stat before open, deliberately: opening a FIFO blocks until a writer appears, so a
    // check made after opening is a check that never runs.
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(EnvironmentReading::Absent);
        }
        Err(error) => return Err(EnvironmentReading::Unreadable(error.to_string())),
    };

    if !metadata.is_file() {
        return Err(EnvironmentReading::Unreadable(format!(
            "{path} is not a regular file, and rastro will not read one that could block or \
             never end"
        )));
    }

    if metadata.len() > LARGEST_ENVIRONMENT_FILE {
        return Err(EnvironmentReading::Unreadable(format!(
            "{path} is {} bytes, past the {LARGEST_ENVIRONMENT_FILE} rastro reads of an \
             environment file",
            metadata.len()
        )));
    }

    let file =
        fs::File::open(path).map_err(|error| EnvironmentReading::Unreadable(error.to_string()))?;

    // Bounded a second time, one byte past the limit, because the size above was read before
    // the open and a file being appended to between the two is exactly the case a bound is
    // for. The same reasoning as the execution seam's output bound.
    let mut text = String::new();
    io::Read::take(file, LARGEST_ENVIRONMENT_FILE + 1)
        .read_to_string(&mut text)
        .map_err(|error| EnvironmentReading::Unreadable(error.to_string()))?;

    if text.len() as u64 > LARGEST_ENVIRONMENT_FILE {
        return Err(EnvironmentReading::Unreadable(format!(
            "{path} grew past the {LARGEST_ENVIRONMENT_FILE} bytes rastro reads of an \
             environment file while it was being read"
        )));
    }

    Ok(parse(&text))
}

/// Reads a file's text the way systemd would.
///
/// Infallible on purpose: systemd sets what it can and ignores the rest, so there is no
/// input for which "this is not an environment file" is the honest answer. A file of prose
/// reports no variables and a count of every line, which is exactly what systemd would do
/// with it.
pub fn parse(text: &str) -> EnvironmentFileContents {
    let mut contents = EnvironmentFileContents::default();

    for line in logical_lines(text) {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match assignment(line) {
            Some((name, value)) => {
                contents.variables.insert(name, value);
            }
            None => contents.ignored_lines += 1,
        }
    }

    contents
}

/// The file's lines, with a backslash-newline joining one to the next.
///
/// The join keeps neither the backslash nor the newline, which is what makes
/// `"first \` / `second"` into `first second`: the space before the backslash is the only
/// separator, and it was already in the text.
///
/// **What decides a continuation is the parity of the trailing backslash run, not the last
/// character.** An odd run ends in a backslash that escapes the newline; an even run is a
/// backslash escaping a backslash, and the line ends. Measured: `V=a\\` followed by `W=b`
/// sets both, while `V=a\` followed by `W=b` sets the single variable `V=aW=b`.
///
/// Getting this wrong is worse than mis-reading a value. Treating an even run as a
/// continuation swallows the following line, so an assignment the service really has
/// disappears from the document with nothing to say it was dropped.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        let trailing_backslashes = line.len() - line.trim_end_matches('\\').len();

        if trailing_backslashes % 2 == 1 {
            // The last backslash escapes the newline, so it and the newline both go.
            current.push_str(&line[..line.len() - 1]);
        } else {
            current.push_str(line);
            lines.push(std::mem::take(&mut current));
        }
    }

    // A file whose last line ends in an odd run continues into nothing.
    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

/// One `NAME=VALUE` line, or nothing where systemd would set nothing.
fn assignment(line: &str) -> Option<(EnvironmentVariableName, String)> {
    let (name, value) = line.split_once('=')?;
    let name = name.trim();

    if !is_systemd_environment_name(name) {
        return None;
    }

    Some((EnvironmentVariableName::new(name).ok()?, unquoted(value)))
}

/// Whether systemd would keep an assignment to this name.
///
/// **A C identifier: a letter or underscore, then letters, digits or underscores.** Measured
/// rather than assumed — `BADNAME-X=x` and `1BAD=y` both reach the process as nothing at all,
/// while `OK_NAME=z` arrives. It is also what drops `export FOO=bar`, since the space makes
/// the name `export FOO`.
///
/// **Checked here and not in [`EnvironmentVariableName`].** That type is shared with the cron
/// collector and deliberately imposes no character rule, because `execve(2)` carries any byte
/// but `=` and NUL and a crontab is not systemd. This grammar is systemd's, so it belongs
/// with systemd's parser; a name it rejects is a line systemd set nothing from, which is the
/// `ignored_lines` count and not a failure.
fn is_systemd_environment_name(name: &str) -> bool {
    let mut characters = name.chars();

    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// The value with its quoting resolved.
///
/// Scanning continues past a closing quote rather than stopping there, because systemd's does:
/// `'a\'b'` comes back as `a\b'`, the trailing `b'` appended to what the quotes held.
///
/// **Trailing whitespace is dropped only where it was outside quotes.** Measured, and it is
/// the distinction the obvious implementation misses: `"  sp  "` keeps both runs of spaces
/// because they are inside the quotes, while `"ab"cd  ` loses its two because they are not.
/// Trimming once at the end gets the first case wrong, which is why the last significant
/// position is tracked as the scan goes rather than reconstructed afterwards.
fn unquoted(value: &str) -> String {
    let mut resolved = String::new();
    let mut characters = value.trim_start().chars();
    let mut quote: Option<char> = None;
    let mut significant = 0;

    while let Some(character) = characters.next() {
        match (character, quote) {
            // A quote opens, or closes the one it matches. Any other quote is a character.
            ('"' | '\'', None) => {
                quote = Some(character);
                // Whatever the quotes go on to hold is kept, empty string included.
                significant = resolved.len();
            }
            ('"' | '\'', Some(open)) if open == character => quote = None,

            // Outside quotes and inside double quotes a backslash escapes; inside single
            // quotes it is a character like any other. Measured, both ways round.
            ('\\', None) => resolved.extend(characters.next()),
            ('\\', Some('"')) => match characters.next() {
                // Measured: `"cost\$5"` reaches the process as `cost$5` and `` "a\`b" `` as
                // `` a`b ``, so the shell's own two expansion characters are escapable here
                // beside the quote and the backslash.
                Some(escaped @ ('"' | '\\' | '$' | '`')) => resolved.push(escaped),
                // Anything else keeps the backslash, which is why `\n` stays two characters.
                // `None` joins this arm rather than getting its own: a logical line cannot end
                // in a backslash, because `logical_lines` has already taken that as a
                // continuation, so the branch would be unreachable and untestable.
                kept => {
                    resolved.push('\\');
                    resolved.extend(kept);
                }
            },

            _ => resolved.push(character),
        }

        if quote.is_some() || !character.is_whitespace() {
            significant = resolved.len();
        }
    }

    resolved.truncate(significant);
    resolved
}
