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
use std::io;

use rastro_collector::EnvironmentVariableName;

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
pub fn read(declared: EnvironmentFile) -> EnvironmentSource {
    let reading = match fs::read_to_string(declared.path.as_str()) {
        Ok(text) => {
            let contents = parse(&text);
            EnvironmentReading::Read {
                variables: contents.variables,
                ignored_lines: contents.ignored_lines,
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => EnvironmentReading::Absent,
        Err(error) => EnvironmentReading::Unreadable(error.to_string()),
    };

    EnvironmentSource { declared, reading }
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
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        match line.strip_suffix('\\') {
            Some(continued) => current.push_str(continued),
            None => {
                current.push_str(line);
                lines.push(std::mem::take(&mut current));
            }
        }
    }

    // A file whose last line ends in a backslash continues into nothing.
    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

/// One `NAME=VALUE` line, or nothing where systemd would set nothing.
///
/// A name is rejected for being empty or holding whitespace, which is the rule that drops
/// `export FOO=bar` — and it is a rule about what `execve(2)` can carry rather than a house
/// style, so it is not tightened further here.
fn assignment(line: &str) -> Option<(EnvironmentVariableName, String)> {
    let (name, value) = line.split_once('=')?;
    let name = name.trim();

    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return None;
    }

    Some((EnvironmentVariableName::new(name).ok()?, unquoted(value)))
}

/// The value with its quoting resolved.
///
/// Scanning continues past a closing quote rather than stopping there, because systemd's does:
/// `'a\'b'` comes back as `a\b'`, the trailing `b'` appended to what the quotes held.
fn unquoted(value: &str) -> String {
    let mut resolved = String::new();
    let mut characters = value.trim_start().chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(character) = characters.next() {
        match (character, quote) {
            // A quote opens, or closes the one it matches. Any other quote is a character.
            ('"' | '\'', None) => quote = Some(character),
            ('"' | '\'', Some(open)) if open == character => quote = None,

            // Outside quotes and inside double quotes a backslash escapes; inside single
            // quotes it is a character like any other. Measured, both ways round.
            ('\\', None) => resolved.extend(characters.next()),
            ('\\', Some('"')) => match characters.next() {
                Some(escaped @ ('"' | '\\')) => resolved.push(escaped),
                // Anything else keeps the backslash, which is why `\n` stays two characters.
                Some(kept) => {
                    resolved.push('\\');
                    resolved.push(kept);
                }
                None => resolved.push('\\'),
            },

            _ => resolved.push(character),
        }
    }

    match quote {
        // Quoted text keeps whatever whitespace it was given; bare text does not.
        Some(_) => resolved,
        None => resolved.trim_end().to_owned(),
    }
}
