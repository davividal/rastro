//! The `systemctl` interface.

use std::collections::{BTreeMap, BTreeSet};

use rastro_collector::CollectionError;

use super::systemctl_unit_files::UnitFileRow;
use super::systemctl_units::UnitRow;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::units::model::{Unit, UnitFile, UnitRegistry, UnitRuntime};
use crate::collectors::units::value_objects::UnitName;

const PROGRAM: &str = "systemctl";

/// Ask for JSON, and the shape becomes rastro's rather than systemd's to format.
///
/// This is the same reasoning that has the packages collector query dpkg through
/// `dpkg-query -f` instead of reading `/var/lib/dpkg/status`, and it matters more here.
/// The table `systemctl` prints by default is whitespace-aligned with a trailing
/// free-text description, so splitting it means guessing where a column ends; unit names
/// reach hundreds of characters for device units, and the alignment shifts with them.
/// `--output=json` removes the guess entirely. systemd 252, which Debian 12 ships,
/// supports it on both subcommands.
const JSON: &str = "--output=json";

/// systemd pages its output by default, and a pager waiting for a terminal is a hang.
///
/// The execution seam already bounds the run and closes stdin, so a pager would hit EOF
/// rather than wedge the box. This is belt and braces on top of that, and it keeps the
/// failure mode out of the picture rather than relying on the bound to catch it.
const NO_PAGER: &str = "--no-pager";

/// Without this, `list-units` reports only what is currently loaded *and* active.
///
/// The inactive units are half the answer: 87 of the 227 loaded units on the development
/// box are `inactive/dead`, and a service that stopped is exactly what a diff should
/// show.
const ALL: &str = "--all";

/// systemd's view of the units, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Systemctl {
    tool: CanonicalTool,
}

impl Systemctl {
    /// Finds systemd's control tool, or reports that this host does not run systemd.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located, so the argument vectors above are
    /// reachable from a test rather than being the one part of the exec route nothing
    /// can observe.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    /// Asks systemd for both halves of the answer and joins them.
    pub fn read(&self) -> Result<UnitRegistry, CollectionError> {
        let files = self.tool.run(&["list-unit-files", JSON, NO_PAGER])?;
        let loaded = self.tool.run(&["list-units", ALL, JSON, NO_PAGER])?;

        Self::join(&files, &loaded)
    }

    /// Combines the two answers into one registry.
    ///
    /// Separate from [`Self::read`] so the whole translation is exercised from a fixture,
    /// with no systemd to run.
    ///
    /// **An outer join, not an inner one.** A name may appear on either side alone, and
    /// both cases are real state: a template such as `autovt@.service` has a file and is
    /// never loaded, while `NetworkManager.service` is loaded `not-found` with no file
    /// behind it. Dropping either population would make the facet quietly incomplete,
    /// which is the one failure this project does not accept.
    pub fn join(files: &str, loaded: &str) -> Result<UnitRegistry, CollectionError> {
        let files = Self::parse_unit_files(files)?;
        let runtimes = Self::parse_units(loaded)?;

        let names: BTreeSet<&UnitName> = files.keys().chain(runtimes.keys()).collect();
        let units = names.into_iter().map(|name| {
            let unit = Unit {
                file: files.get(name).cloned(),
                runtime: runtimes.get(name).cloned(),
            };

            (name.clone(), unit)
        });

        Ok(UnitRegistry::new(units))
    }

    fn parse_unit_files(output: &str) -> Result<BTreeMap<UnitName, UnitFile>, CollectionError> {
        let rows: Vec<UnitFileRow> = Self::decode(output, "list-unit-files")?;

        Self::keyed(rows.iter().map(UnitFileRow::to_unit_file), "unit file")
    }

    fn parse_units(output: &str) -> Result<BTreeMap<UnitName, UnitRuntime>, CollectionError> {
        let rows: Vec<UnitRow> = Self::decode(output, "list-units")?;

        Self::keyed(rows.iter().map(UnitRow::to_runtime), "loaded unit")
    }

    /// Files translated rows under their name, refusing a repeat.
    ///
    /// systemd enforces one unit per name, so a repeat means rastro misread the output,
    /// and keeping the last of two would drop a unit from a document claiming to be
    /// complete. The same rule every keyed collection here follows.
    fn keyed<T>(
        rows: impl Iterator<Item = Result<(UnitName, T), CollectionError>>,
        kind: &str,
    ) -> Result<BTreeMap<UnitName, T>, CollectionError> {
        let mut keyed = BTreeMap::new();

        for row in rows {
            let (name, value) = row?;
            if keyed.insert(name.clone(), value).is_some() {
                return Err(CollectionError::new(format!(
                    "systemd reported the {kind} {:?} twice, so the output was misread",
                    name.as_str()
                )));
            }
        }

        Ok(keyed)
    }

    /// Reads one of systemd's JSON arrays, naming the subcommand if it will not parse.
    ///
    /// The message names the subcommand rather than quoting the output, which for
    /// `list-units` is thirty kilobytes and would bury the reason it failed.
    fn decode<T: serde::de::DeserializeOwned>(
        output: &str,
        subcommand: &str,
    ) -> Result<Vec<T>, CollectionError> {
        serde_json::from_str(output).map_err(|error| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM} {subcommand}` reported as JSON: {error}"
            ))
        })
    }
}
