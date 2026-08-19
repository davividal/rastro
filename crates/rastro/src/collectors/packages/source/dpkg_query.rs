//! The `dpkg-query` interface.

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::packages::model::{InstallationStatus, Package, PackageSet};
use crate::collectors::packages::value_objects::{
    Architecture, ErrorFlag, InstallationState, PackageName, PackageVersion, SelectionState,
};

const PROGRAM: &str = "dpkg-query";

/// The output format, which rastro chooses rather than accepts.
///
/// This is why dpkg is queried through its tool instead of by reading
/// `/var/lib/dpkg/status`: `-f` makes the shape ours, where the status file is a
/// multi-line RFC822-ish format dpkg's own documentation says not to parse.
///
/// The status is asked for as three words rather than as `${db:Status-Abbrev}`, so dpkg
/// decodes its own vocabulary and rastro maintains no alphabet of status letters.
const FORMAT: &str = concat!(
    "-f=${binary:Package}\t${Version}\t${Architecture}\t",
    "${db:Status-Want}\t${db:Status-Status}\t${db:Status-Eflag}\n"
);

/// dpkg's package list as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpkgQuery {
    tool: CanonicalTool,
}

impl DpkgQuery {
    /// Finds dpkg's query tool, or reports that this host does not use dpkg.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    ///
    /// Every other source takes its interface this way, and without it the pairing of `-W` with
    /// the query format was the one line in the exec route that nothing could observe: changing
    /// that format would leave every test passing and every real Debian box failing.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    /// Asks dpkg for every package it knows, and translates the answer.
    pub fn read(&self) -> Result<PackageSet, CollectionError> {
        Self::parse(&self.tool.run(&["-W", FORMAT])?)
    }

    /// Translates the tool's output into the model.
    ///
    /// Separate from [`Self::read`] so the whole grammar is exercised from a fixture, with
    /// no dpkg to run.
    ///
    /// Packages dpkg knows but has not fully installed are kept: `config-files` for one
    /// removed without purging, `half-installed` and `half-configured` for one caught mid
    /// operation. Those are real differences between two runs, and the interesting ones.
    ///
    /// Not every state, though, and the difference was measured rather than assumed:
    /// `dpkg-query -W` with no pattern silently omits `not-installed` rows, so purging a
    /// package removes its key from the facet instead of showing it as not installed. Still
    /// diffable, and claiming otherwise would be a guarantee this query does not give.
    pub fn parse(output: &str) -> Result<PackageSet, CollectionError> {
        let packages = output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(Self::parse_line)
            .collect::<Result<Vec<(PackageName, Package)>, CollectionError>>()?;

        PackageSet::new(packages)
    }

    fn parse_line(line: &str) -> Result<(PackageName, Package), CollectionError> {
        let [name, version, architecture, selection, state, error_flag] =
            line.split('\t').collect::<Vec<&str>>()[..]
        else {
            return Err(CollectionError::new(format!(
                "expected six tab-separated fields from {PROGRAM}: {line:?}"
            )));
        };

        let package = Package {
            version: PackageVersion::new(version)?,
            architecture: Architecture::new(architecture)?,
            status: Some(InstallationStatus {
                selection: SelectionState::new(selection)?,
                state: InstallationState::new(state)?,
                error_flag: ErrorFlag::new(error_flag)?,
            }),
        };

        Ok((PackageName::new(name)?, package))
    }
}
