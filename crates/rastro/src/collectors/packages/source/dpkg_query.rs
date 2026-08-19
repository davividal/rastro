//! The `dpkg-query` interface.

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::packages::model::{InstallationStatus, Package, PackageSet};
use crate::collectors::packages::value_objects::{
    Architecture, ErrorFlag, InstallationState, PackageName, PackageVersion, SelectionState,
};

const PROGRAM: &str = "dpkg-query";

/// Where dpkg keeps its tool, tried before a `PATH` search because rastro runs as root.
const WELL_KNOWN: [&str; 2] = ["/usr/bin/dpkg-query", "/bin/dpkg-query"];

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
        CanonicalTool::located(PROGRAM, &WELL_KNOWN).map(|tool| Self { tool })
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
    /// Packages dpkg knows but has not installed are kept, `un` and `rc` alike: absence is
    /// state, and purged versus removed-but-configured is a real difference between two
    /// runs.
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
