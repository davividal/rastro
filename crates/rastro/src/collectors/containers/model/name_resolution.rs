//! How a container resolves names.

use std::collections::BTreeMap;

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::inet::IpAddress;

/// The resolver a container was given, and the names it was told about directly.
///
/// **Which resolver a container uses decides what it can reach**, and an `--add-host` entry
/// is a name that resolves nowhere else on the box: not in the host's `/etc/hosts`, not in
/// any DNS the host uses. A container pointed at a different resolver than its neighbours is
/// the kind of thing that is invisible until something cannot connect.
///
/// The lists are kept in the order they were given, because a resolver list is ordered: the
/// first server is the one asked first, and sorting them would change what the container
/// does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NameResolution {
    pub servers: Vec<IpAddress>,
    pub searches: Vec<NonEmptyText>,
    pub options: Vec<NonEmptyText>,
    /// The `--add-host` entries, keyed by name.
    pub hosts: BTreeMap<NonEmptyText, IpAddress>,
}

impl From<&NameResolution> for Observation {
    fn from(resolution: &NameResolution) -> Self {
        Observation::object([
            (
                "hosts",
                Observation::object(
                    resolution
                        .hosts
                        .iter()
                        .map(|(name, address)| (name.as_str(), Observation::from(address))),
                ),
            ),
            (
                "options",
                Observation::list(
                    resolution
                        .options
                        .iter()
                        .map(|option| Observation::text(option.as_str())),
                ),
            ),
            (
                "searches",
                Observation::list(
                    resolution
                        .searches
                        .iter()
                        .map(|search| Observation::text(search.as_str())),
                ),
            ),
            (
                "servers",
                Observation::list(resolution.servers.iter().map(Observation::from)),
            ),
        ])
    }
}
