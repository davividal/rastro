//! One path rule inside a virtual host.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::nginx::model::{AccessRule, Authentication, PassTarget};
use crate::collectors::nginx::value_objects::LocationPattern;

/// A `location` block: what it matches, where it sends, and who may reach it.
///
/// **Kept in the order written**, because nginx's matching depends on it: regular-expression
/// locations are tried in the order they appear, so moving one past another changes which
/// requests it serves without changing a character inside either.
///
/// Nested locations are kept nested, which is how nginx reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub pattern: LocationPattern,
    pub pass: Option<PassTarget>,
    pub root: Option<NonEmptyText>,
    pub access: Vec<AccessRule>,
    pub authentication: Option<Authentication>,
    pub locations: Vec<Location>,
}

impl From<&Location> for Observation {
    fn from(location: &Location) -> Self {
        Observation::object([
            (
                "access",
                Observation::list(location.access.iter().map(Observation::from)),
            ),
            (
                "authentication",
                location
                    .authentication
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "locations",
                Observation::list(location.locations.iter().map(Observation::from)),
            ),
            (
                "pass",
                location
                    .pass
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("pattern", Observation::from(&location.pattern)),
            (
                "root",
                location
                    .root
                    .as_ref()
                    .map_or_else(Observation::null, |root| Observation::text(root.as_str())),
            ),
        ])
    }
}
