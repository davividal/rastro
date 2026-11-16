//! The access one or more security flavours are given.

use rastro_collector::Observation;

use crate::collectors::nfs::value_objects::{ExportOption, SecurityFlavour};

/// One `sec=` option of an export and the access options that follow it.
///
/// nfs-utils lets each flavour carry its own `rw`, `ro` and squashing, and writes them after the
/// `sec=` they belong to. A client is given the group of the flavour it mounts with, so an
/// export that is `rw` for `krb5` and `ro` for `sys` reads as two groups, not as one export
/// that is both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityGroup {
    /// In the order the server offers them, which is the order a client negotiates in.
    pub flavours: Vec<SecurityFlavour>,
    pub options: Vec<ExportOption>,
}

impl From<&SecurityGroup> for Observation {
    fn from(group: &SecurityGroup) -> Self {
        Observation::object([
            (
                "flavours",
                Observation::sequence(group.flavours.iter().map(Observation::from)),
            ),
            (
                "options",
                Observation::set(group.options.iter().map(Observation::from)),
            ),
        ])
    }
}
