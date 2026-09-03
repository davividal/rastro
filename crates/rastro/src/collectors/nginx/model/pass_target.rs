//! Where a location hands a request on to.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::nginx::value_objects::PassKind;

/// A `*_pass` directive: which protocol, and what it names.
///
/// The target is kept as written rather than resolved against the pools. `proxy_pass
/// http://backend;` may name an `upstream` block, a host, or a variable that is only known
/// per request, and the three are told apart by looking at the upstreams in the same facet.
/// Resolving it here would turn an unresolvable case into either a lie or a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassTarget {
    pub kind: PassKind,
    pub target: NonEmptyText,
}

impl From<&PassTarget> for Observation {
    fn from(pass: &PassTarget) -> Self {
        Observation::object([
            ("kind", Observation::from(&pass.kind)),
            ("target", Observation::text(pass.target.as_str())),
        ])
    }
}
