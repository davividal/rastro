//! The `authorized_keys` grammar.
//!
//! `[options] keytype base64 [comment]`, one key per line.

use rastro_collector::CollectionError;

use crate::collectors::ssh_access::model::AuthorizedKey;
use crate::collectors::ssh_access::value_objects::{KeyComment, KeyOption, KeyType, PublicKey};

/// Translates one file into keys.
///
/// Blank lines and `#` comments are skipped, as OpenSSH skips them.
pub fn parse(contents: &str) -> Result<Vec<AuthorizedKey>, CollectionError> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(parse_line)
        .collect()
}

/// One line.
///
/// **Whether the line begins with options is the one genuinely ambiguous thing here**, and it
/// is settled by looking at the first field rather than by counting. If the line begins with
/// something shaped like a key type there are no options; otherwise everything up to the first
/// *unquoted* whitespace is the options list. Counting fields instead fails on the common case
/// of a key with no comment, which has the same field count as an options-bearing key with one.
///
/// **The options field is found by scanning with a quote latch, not by splitting on the first
/// space**, and a test is what forced that: `command="/usr/bin/thing --list a,b"` is a single
/// option containing a space, so splitting on whitespace cuts it in half and loses both the
/// rest of the command and the option that followed it. That is the difference between
/// reporting a key restricted to one command and reporting one restricted to something else
/// entirely.
fn parse_line(line: &str) -> Result<AuthorizedKey, CollectionError> {
    let (options, rest) = match options_field(line) {
        Some((field, rest)) => (split_options(field)?, rest),
        None => (Vec::new(), line),
    };

    let (key_type, rest) = rest.split_once(char::is_whitespace).ok_or_else(|| {
        CollectionError::new(format!(
            "an authorized_keys line needs a type and a key: {}",
            elided(line)
        ))
    })?;

    // The comment is whatever follows the key, untokenised: OpenSSH does not split it, so a
    // comment with spaces in it is one comment.
    let (key, comment) = match rest.trim_start().split_once(char::is_whitespace) {
        Some((key, comment)) => (key, comment.trim()),
        None => (rest.trim(), ""),
    };

    let mut options = options;
    options.sort();

    Ok(AuthorizedKey {
        key_type: KeyType::new(key_type)?,
        key: PublicKey::new(key)?,
        comment: KeyComment::new(comment),
        options,
    })
}

/// The options at the front of a line, and what follows them.
///
/// `None` when the line begins with a key type, which is the unrestricted case and the common
/// one. Otherwise the field runs to the first whitespace that is not inside quotes.
fn options_field(line: &str) -> Option<(&str, &str)> {
    let first = line.split_whitespace().next()?;
    if KeyType::looks_like_one(first) {
        return None;
    }

    let mut inside_quotes = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => inside_quotes = !inside_quotes,
            _ if character.is_whitespace() && !inside_quotes => {
                return Some((&line[..index], line[index..].trim_start()));
            }
            _ => {}
        }
    }

    // A line that is nothing but an options list has no key on it, which the caller reports.
    Some((line, ""))
}

/// Splits the options field on the commas that separate options, not the ones inside a quoted
/// value.
///
/// **The same hazard the mount collector's option splitter has, and for the same reason.**
/// `command="/usr/bin/thing --list a,b"` is one option, and a naive split invents two that
/// nobody authorised. Quoting here is simpler than in a mount option, though: OpenSSH's own
/// parser treats a `"` as opening a quoted value and the next `"` as closing it, with no
/// escaping, so a latch is what it does rather than a guess.
fn split_options(field: &str) -> Result<Vec<KeyOption>, CollectionError> {
    let mut options = Vec::new();
    let mut start = 0;
    let mut inside_quotes = false;

    for (index, character) in field.char_indices() {
        match character {
            '"' => inside_quotes = !inside_quotes,
            ',' if !inside_quotes => {
                push_option(&mut options, &field[start..index])?;
                start = index + 1;
            }
            _ => {}
        }
    }
    push_option(&mut options, &field[start..])?;

    Ok(options)
}

/// Adds an option, skipping an empty one.
///
/// A trailing comma is legal and produces nothing, which is different from an option named
/// nothing.
fn push_option(options: &mut Vec<KeyOption>, option: &str) -> Result<(), CollectionError> {
    let option = option.trim();
    if option.is_empty() {
        return Ok(());
    }

    options.push(KeyOption::new(option)?);

    Ok(())
}

/// How much of a malformed line a failure quotes.
const QUOTED: usize = 40;

/// The head of a line, for a failure message.
///
/// Shortened rather than quoted whole. A public key is not a secret, but an eighty-character
/// base64 blob on stderr buries the reason the line failed.
fn elided(line: &str) -> String {
    let head: String = line.chars().take(QUOTED).collect();

    if head.chars().count() < line.chars().count() {
        return format!("{head}...");
    }

    head
}
