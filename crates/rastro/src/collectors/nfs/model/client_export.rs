//! One path, exported to one client.

use rastro_collector::Observation;

use super::security_group::SecurityGroup;
use crate::collectors::nfs::value_objects::ExportOption;

/// The options an export applies to one client, with every default filled in.
///
/// Pathless and clientless on purpose: both are the keys this is filed under in the
/// [`ExportTable`](super::export_table::ExportTable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientExport {
    /// The options that hold whichever flavour a client uses.
    pub options: Vec<ExportOption>,

    /// In the order `exportfs` wrote them, which is the order the flavours are offered in.
    pub security: Vec<SecurityGroup>,
}

impl From<&ClientExport> for Observation {
    fn from(export: &ClientExport) -> Self {
        Observation::object([
            (
                "options",
                Observation::set(export.options.iter().map(Observation::from)),
            ),
            (
                "security",
                Observation::sequence(export.security.iter().map(Observation::from)),
            ),
        ])
    }
}
