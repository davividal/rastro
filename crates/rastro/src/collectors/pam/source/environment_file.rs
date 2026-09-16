//! `/etc/environment`, as `pam_env` reads it.
//!
//! **This file looks like a unit's `EnvironmentFile=` and is read by a different program with
//! different rules.** Every rule below was measured against `libpam-modules` 1.7.0 on Debian
//! 13 — a probe file written, a PAM login performed, the resulting environment read back —
//! and three of them are the exact opposite of the systemd reader in
//! [`units`](crate::collectors::units):
//!
//! | line | `pam_env` | systemd |
//! |---|---|---|
//! | `export V=x` | sets `V` | sets nothing, the name holds a space |
//! | `V=x␠␠␠` | keeps the spaces | trims them |
//! | `V=a # b` | truncates at the `#` | `# b` is part of the value |
//!
//! Sharing one parser between the two would therefore be wrong in both directions, which is
//! why there are two. What they do share is the vocabulary: both key their result by
//! [`EnvironmentVariableName`].

use std::collections::BTreeMap;

use rastro_collector::EnvironmentVariableName;

/// What `/etc/environment` sets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentAssignments {
    /// Keyed by name, so a diff has one order and a repeat has one answer. `pam_env` takes
    /// the last of a repeated name, which a map gives for free.
    pub variables: BTreeMap<EnvironmentVariableName, String>,
    /// How many lines `pam_env` would set nothing from.
    ///
    /// A comment or a blank line is not counted: those are not lines that failed, they are
    /// lines that say nothing. What is counted is an assignment that looks like one and is
    /// not — `V = x`, whose name holds a space, being the one an operator writes by mistake.
    pub ignored_lines: usize,
}

/// Reads the file's text the way `pam_env` would.
///
/// Infallible for the same reason the systemd reader is: `pam_env` sets what it can and
/// ignores the rest, so there is no input for which "this is not an environment file" is the
/// honest answer.
pub fn parse(text: &str) -> EnvironmentAssignments {
    let mut assignments = EnvironmentAssignments::default();

    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Before anything else, and before the quotes are looked at: `V="a # b"` really does
        // come back as `a `, so the truncation cannot be deferred until the value is parsed.
        let line = match line.split_once('#') {
            Some((before, _)) => before,
            None => line,
        };

        // Exactly one space, because a tab leaves the name as `export\tV` and `pam_env` sets
        // nothing from it.
        let line = line.strip_prefix("export ").unwrap_or(line);

        match assignment(line) {
            Some((name, value)) => {
                assignments.variables.insert(name, value);
            }
            None => assignments.ignored_lines += 1,
        }
    }

    assignments
}

/// One `NAME=VALUE` line, or nothing where `pam_env` would set nothing.
///
/// The name is **not** trimmed, deliberately. `V = x` sets nothing on a real box because the
/// name is `V ` and that is not an identifier, so trimming here would report a variable the
/// session does not have.
fn assignment(line: &str) -> Option<(EnvironmentVariableName, String)> {
    let (name, value) = line.split_once('=')?;

    if !is_identifier(name) {
        return None;
    }

    Some((EnvironmentVariableName::new(name).ok()?, unquoted(value)))
}

/// A C identifier: a letter or underscore, then letters, digits or underscores.
///
/// Measured rather than assumed: `BADNAME-X=x` and `1BAD=y` both reach a session as nothing
/// at all, while `OK_NAME=z` arrives.
fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();

    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// The value with its surrounding quotes taken off.
///
/// **Asymmetric, and that is measured rather than a simplification.** A leading quote is
/// dropped wherever it appears; a trailing one only when it is the last character, so
/// `"ab"␠␠` keeps its closing quote and its spaces. Nothing inside is unescaped: `pam_env`
/// hands the bytes through, so a backslash stays a backslash.
///
/// Whitespace is never trimmed, on either side. That is the third place this reader parts
/// company with systemd's, and the reason a value here can begin with a space.
fn unquoted(value: &str) -> String {
    let opened = ['"', '\''];

    let Some(quote) = value.chars().next().filter(|first| opened.contains(first)) else {
        return value.to_owned();
    };

    let inside = &value[quote.len_utf8()..];

    match inside.strip_suffix(quote) {
        Some(closed) => closed.to_owned(),
        None => inside.to_owned(),
    }
}
