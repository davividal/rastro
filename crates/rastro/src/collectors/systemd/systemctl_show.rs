//! The `systemctl show` property dump.
//!
//! One group of `Property=value` lines per unit, groups separated by a blank line. Shared
//! rather than owned by the units collector, because the exporters facet reads the same
//! dump to find how a telemetry agent was started.
//!
//! # Two shapes this parser exists to survive
//!
//! - **The properties come back in systemd's order, not the order they were asked for.**
//!   On systemd 252, `-p Id -p ExecStartEx` prints `ExecStartEx` first. Reading the group
//!   positionally would work today and break on any reordering, so both keys are searched
//!   for by name.
//! - **A unit with several `ExecStart=` lines gets several `ExecStartEx=` lines**, one per
//!   command, rather than one line carrying several groups. Measured with a throwaway unit,
//!   because nothing on the development box has two.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, EnvironmentVariableName};
use serde::Deserialize;

use super::exec_start::ExecStart;
use super::shown_unit::ShownUnit;
use super::unit_name::UnitName;

const ID: &str = "Id=";
const EXEC_START: &str = "ExecStartEx=";
const ENVIRONMENT: &str = "Environment=";

/// What separates the fields inside one command's braced group.
const FIELDS: &str = " ; ";

const PATH_FIELD: &str = "path=";
const ARGV_FIELD: &str = "argv[]=";

/// Reads the dump into the commands each unit starts.
///
/// A unit that starts nothing maps to an empty list rather than being dropped: systemd
/// prints its `Id=` line either way, and a unit rastro saw and did not report would be a
/// hole in a document claiming to be complete.
pub fn parse(dump: &str) -> Result<BTreeMap<UnitName, ShownUnit>, CollectionError> {
    let mut shown = BTreeMap::new();

    for group in dump.split("\n\n") {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }

        let (name, unit) = parse_group(group)?;
        if shown.insert(name.clone(), unit).is_some() {
            return Err(CollectionError::new(format!(
                "`systemctl show` reported the unit {:?} twice, so the output was misread",
                name.as_str()
            )));
        }
    }

    Ok(shown)
}

fn parse_group(group: &str) -> Result<(UnitName, ShownUnit), CollectionError> {
    let name = group
        .lines()
        .find_map(|line| line.strip_prefix(ID))
        .ok_or_else(|| {
            CollectionError::new(format!(
                "`systemctl show` printed a group with no {ID:?} line, so what it describes \
                 cannot be named: {group:?}"
            ))
        })?;

    let exec_start = group
        .lines()
        .filter_map(|line| line.strip_prefix(EXEC_START))
        .map(parse_command)
        .collect::<Result<Vec<ExecStart>, CollectionError>>()?;

    // One line, however many the unit file spread the setting over, so the last match wins
    // nothing and a missing key is simply an empty environment.
    let environment = match group
        .lines()
        .find_map(|line| line.strip_prefix(ENVIRONMENT))
    {
        Some(line) => parse_environment(line)?,
        None => BTreeMap::new(),
    };

    Ok((
        UnitName::new(name)?,
        ShownUnit {
            exec_start,
            environment,
        },
    ))
}

/// The `Environment=` line's entries, keyed by name.
///
/// A repeated name is refused rather than resolved. systemd has already merged the unit and
/// its drop-ins by the time it prints this, so two entries sharing a name means this parser
/// split the line wrongly, and quietly keeping one of them would hide the misreading behind
/// a plausible answer.
fn parse_environment(
    line: &str,
) -> Result<BTreeMap<EnvironmentVariableName, String>, CollectionError> {
    let mut environment = BTreeMap::new();

    for entry in entries_of(line)? {
        let (name, value) = entry.split_once('=').ok_or_else(|| {
            CollectionError::new(format!(
                "`systemctl show` printed an `Environment=` entry with no `=` in it, so it \
                 names no variable: {entry:?}"
            ))
        })?;

        if environment
            .insert(EnvironmentVariableName::new(name)?, value.to_owned())
            .is_some()
        {
            return Err(CollectionError::new(format!(
                "`systemctl show` reported {name:?} twice on one `Environment=` line, so the \
                 line was misread"
            )));
        }
    }

    Ok(environment)
}

/// Splits the line into its `NAME=VALUE` entries, unescaping each.
///
/// **Entries are space-separated and quoted only when they need to be**, and the quotes wrap
/// the whole entry rather than the value: `"SPACED=two words"`. Inside or outside them
/// systemd C-escapes what it prints, so the wire spelling is not the value — `a\nb` on this
/// line is three characters in the process, which was measured by reading `/usr/bin/env` out
/// of a started unit rather than assumed.
fn entries_of(line: &str) -> Result<Vec<String>, CollectionError> {
    let mut entries = Vec::new();
    let mut entry = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut characters = line.chars();

    while let Some(character) = characters.next() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ' ' if !quoted => {
                if started {
                    entries.push(std::mem::take(&mut entry));
                    started = false;
                }
            }
            '\\' => {
                entry.push(unescaped(&mut characters, line)?);
                started = true;
            }
            _ => {
                entry.push(character);
                started = true;
            }
        }
    }

    if quoted {
        return Err(CollectionError::new(format!(
            "`systemctl show` printed an `Environment=` line whose quoting does not close, so \
             where one variable ends cannot be told: {line:?}"
        )));
    }

    if started {
        entries.push(entry);
    }

    Ok(entries)
}

/// What one backslash escape stands for.
///
/// The table is systemd's `cescape`, plus the `$` it escapes here specifically so a value is
/// not mistaken for a specifier. Anything outside it is refused rather than guessed: an
/// unknown escape means this table has fallen behind systemd's, and inventing a character
/// would put one in the document that is not in the process.
fn unescaped(characters: &mut std::str::Chars<'_>, line: &str) -> Result<char, CollectionError> {
    let escaped = characters.next().ok_or_else(|| {
        CollectionError::new(format!(
            "`systemctl show` printed an `Environment=` line ending in a lone backslash: \
             {line:?}"
        ))
    })?;

    match escaped {
        'a' => Ok('\u{7}'),
        'b' => Ok('\u{8}'),
        'f' => Ok('\u{c}'),
        'n' => Ok('\n'),
        'r' => Ok('\r'),
        't' => Ok('\t'),
        'v' => Ok('\u{b}'),
        '\\' | '"' | '\'' | '$' => Ok(escaped),
        'x' => hex_escape(characters, line),
        other => Err(CollectionError::new(format!(
            "`systemctl show` printed the escape {other:?} in an `Environment=` line, which \
             rastro cannot read: {line:?}"
        ))),
    }
}

/// The `\xNN` form, which systemd uses for a byte it will not print.
///
/// **Refused above 0x7f rather than decoded.** One escaped byte is half a character in any
/// multi-byte encoding, and the value it belongs to is a Rust `String`. systemd leaves valid
/// UTF-8 alone — `héllo` came back unescaped — so a high byte here means the value was never
/// text, and a lossy stand-in would be a value the box does not have.
fn hex_escape(characters: &mut std::str::Chars<'_>, line: &str) -> Result<char, CollectionError> {
    let digits: String = characters.by_ref().take(2).collect();
    let byte = u8::from_str_radix(&digits, 16).map_err(|_| {
        CollectionError::new(format!(
            "`systemctl show` printed the escape \"\\x{digits}\" in an `Environment=` line, \
             which is not two hex digits: {line:?}"
        ))
    })?;

    match byte.is_ascii() {
        true => Ok(char::from(byte)),
        false => Err(CollectionError::new(format!(
            "`systemctl show` escaped the byte 0x{byte:02x} in an `Environment=` line, so that \
             value is not text and rastro will not guess at one: {line:?}"
        ))),
    }
}

/// One `{ path=… ; argv[]=… ; … }` group.
///
/// The fields after `argv[]` are systemd's runtime bookkeeping — a pid, an exit status, the
/// times it last ran — rather than configuration, which is why two of nine are read. They
/// are also the volatile ones, so leaving them out is what keeps this facet stable across
/// two runs of an unchanged box.
fn parse_command(command: &str) -> Result<ExecStart, CollectionError> {
    let inside = command
        .trim()
        .strip_prefix('{')
        .and_then(|command| command.strip_suffix('}'))
        .ok_or_else(|| {
            CollectionError::new(format!(
                "`systemctl show` printed an {EXEC_START:?} value that is not a braced \
                 group: {command:?}"
            ))
        })?;

    ExecStart::new(field(inside, PATH_FIELD)?, field(inside, ARGV_FIELD)?)
}

fn field<'a>(fields: &'a str, name: &str) -> Result<&'a str, CollectionError> {
    fields
        .split(FIELDS)
        .find_map(|field| field.trim().strip_prefix(name))
        .ok_or_else(|| {
            CollectionError::new(format!(
                "`systemctl show` reported an ExecStart with no {name:?} field: {fields:?}"
            ))
        })
}

/// One row of `systemctl list-units --output=json`, narrowed to the name.
///
/// Only the name, because this exists to build the argument vector for a `show` that then
/// asks for everything else. A collector wanting the load and active states parses its own
/// richer row.
#[derive(Debug, Deserialize)]
struct UnitRow {
    unit: String,
}

/// The names of the loaded units, in systemd's order.
///
/// Needed because `systemctl show` has to be given every unit by name: its glob form
/// answers for a subset with no error and no warning, which on the development box is 47
/// of 109 service units.
pub fn unit_names(loaded: &str) -> Result<Vec<String>, CollectionError> {
    let rows: Vec<UnitRow> = serde_json::from_str(loaded).map_err(|error| {
        CollectionError::new(format!(
            "could not read what `systemctl list-units` reported as JSON: {error}"
        ))
    })?;

    Ok(rows.into_iter().map(|row| row.unit).collect())
}
