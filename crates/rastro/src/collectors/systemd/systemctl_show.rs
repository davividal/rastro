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

use rastro_collector::CollectionError;
use serde::Deserialize;

use super::exec_start::ExecStart;
use super::unit_name::UnitName;

const ID: &str = "Id=";
const EXEC_START: &str = "ExecStartEx=";

/// What separates the fields inside one command's braced group.
const FIELDS: &str = " ; ";

const PATH_FIELD: &str = "path=";
const ARGV_FIELD: &str = "argv[]=";

/// Reads the dump into the commands each unit starts.
///
/// A unit that starts nothing maps to an empty list rather than being dropped: systemd
/// prints its `Id=` line either way, and a unit rastro saw and did not report would be a
/// hole in a document claiming to be complete.
pub fn parse(dump: &str) -> Result<BTreeMap<UnitName, Vec<ExecStart>>, CollectionError> {
    let mut shown = BTreeMap::new();

    for group in dump.split("\n\n") {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }

        let (name, starts) = parse_group(group)?;
        if shown.insert(name.clone(), starts).is_some() {
            return Err(CollectionError::new(format!(
                "`systemctl show` reported the unit {:?} twice, so the output was misread",
                name.as_str()
            )));
        }
    }

    Ok(shown)
}

fn parse_group(group: &str) -> Result<(UnitName, Vec<ExecStart>), CollectionError> {
    let name = group
        .lines()
        .find_map(|line| line.strip_prefix(ID))
        .ok_or_else(|| {
            CollectionError::new(format!(
                "`systemctl show` printed a group with no {ID:?} line, so what it describes \
                 cannot be named: {group:?}"
            ))
        })?;

    let starts = group
        .lines()
        .filter_map(|line| line.strip_prefix(EXEC_START))
        .map(parse_command)
        .collect::<Result<Vec<ExecStart>, CollectionError>>()?;

    Ok((UnitName::new(name)?, starts))
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
