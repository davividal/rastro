//! Which build of an agent this is.

use rastro_collector::Observation;

use crate::collectors::exporters::value_objects::{BuildRevision, ExporterVersion};

/// An agent's own account of itself.
///
/// Both halves, because either alone is a partial answer: the version says which release
/// was intended and the revision says which build actually landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExporterBuild {
    pub version: ExporterVersion,
    pub revision: BuildRevision,
}

impl From<&ExporterBuild> for Observation {
    fn from(build: &ExporterBuild) -> Self {
        Observation::object([
            ("revision", Observation::from(&build.revision)),
            ("version", Observation::from(&build.version)),
        ])
    }
}
