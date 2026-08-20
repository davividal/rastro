//! What dpkg says about a package's state.

use rastro_collector::Observation;

use crate::collectors::packages::value_objects::{ErrorFlag, InstallationState, SelectionState};

/// The three-part status dpkg keeps for every package it knows.
///
/// Only dpkg reports one. apk's database lists what is installed and nothing else, so a
/// package from there carries no status rather than a fabricated one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationStatus {
    pub selection: SelectionState,
    pub state: InstallationState,
    pub error_flag: ErrorFlag,
}

impl From<&InstallationStatus> for Observation {
    fn from(status: &InstallationStatus) -> Self {
        Observation::object([
            ("selection", Observation::text(status.selection.as_str())),
            ("state", Observation::text(status.state.as_str())),
            ("error_flag", Observation::text(status.error_flag.as_str())),
        ])
    }
}
