//! The `systemctl` interface.

use std::collections::{BTreeMap, BTreeSet};

use rastro_collector::CollectionError;

use super::systemctl_unit_files::UnitFileRow;
use super::systemctl_units::UnitRow;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::systemd::{ExecStart, UnitName, systemctl_show};
use crate::collectors::units::model::{Unit, UnitFile, UnitRegistry, UnitRuntime};

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

/// Ask what each unit starts. `list-units` does not carry it, and it is the half of the
/// answer that says which binary an enabled service actually amounts to.
const SHOW: &str = "show";

const ID_PROPERTY: &str = "--property=Id";
const EXEC_START_PROPERTY: &str = "--property=ExecStartEx";

/// Everything after this is a unit name, however much it looks like an option.
///
/// **Not optional, and not defensive programming.** systemd's root slice and root mount are
/// named `-.slice` and `-.mount`, both loaded on every box, and `systemctl` rejects them as
/// bare arguments with `invalid option -- '.'`. Its own hint is to use `--`, which is what
/// this is.
const END_OF_OPTIONS: &str = "--";

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

    /// Asks systemd for every half of the answer and joins them.
    pub fn read(&self) -> Result<UnitRegistry, CollectionError> {
        let files = self.tool.run(&["list-unit-files", JSON, NO_PAGER])?;
        let loaded = self.tool.run(&["list-units", ALL, JSON, NO_PAGER])?;
        let shown = self.show(Self::parse_units(&loaded)?.keys())?;

        Self::join(&files, &loaded, &shown)
    }

    /// Asks what each loaded unit starts, naming every unit rather than globbing.
    ///
    /// **The glob is the trap, and it is silent.** `systemctl show '*.service'` answers for
    /// 47 of the 109 service units `list-units --all` reports on the development box, with
    /// no error and no warning: it matches only what is currently loaded in the sense the
    /// glob means, and the units it drops are ordinary ones. Naming every unit is what makes
    /// this population the same as the one the runtime rows came from, which is the only way
    /// the join below cannot quietly lose a unit.
    ///
    /// The loaded units, not the unit files. A file systemd has never resolved has no
    /// effective `ExecStart=` to report, and reading the file itself would mean
    /// re-implementing drop-in resolution — the exact thing asking systemd avoids.
    fn show<'a>(
        &self,
        names: impl Iterator<Item = &'a UnitName>,
    ) -> Result<String, CollectionError> {
        let names: Vec<&str> = names.map(UnitName::as_str).collect();

        // A box with no loaded units is not a box to ask about them: `systemctl show` with
        // no unit named answers about the manager itself, which is a different question.
        if names.is_empty() {
            return Ok(String::new());
        }

        let mut arguments = vec![
            SHOW,
            ID_PROPERTY,
            EXEC_START_PROPERTY,
            NO_PAGER,
            END_OF_OPTIONS,
        ];
        arguments.extend(names);

        self.tool.run(&arguments)
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
    pub fn join(files: &str, loaded: &str, shown: &str) -> Result<UnitRegistry, CollectionError> {
        let files = Self::parse_unit_files(files)?;
        let runtimes = Self::parse_units(loaded)?;
        let starts: BTreeMap<UnitName, Vec<ExecStart>> = systemctl_show::parse(shown)?;

        let names: BTreeSet<&UnitName> = files.keys().chain(runtimes.keys()).collect();
        let units = names.into_iter().map(|name| {
            let unit = Unit {
                file: files.get(name).cloned(),
                runtime: runtimes.get(name).cloned(),
                exec_start: starts.get(name).cloned().unwrap_or_default(),
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
