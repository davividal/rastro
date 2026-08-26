//! Where an agent was told to serve.

use rastro_collector::Observation;

use crate::collectors::inet::{InetHost, PortNumber};

/// The address an agent's flags configure it to listen on.
///
/// **Configured, not bound.** Whether anything is actually listening there is the sockets
/// facet's answer, read from the kernel. Keeping the two apart is what lets them disagree,
/// and a disagreement is a finding: an agent configured for 9100 with nothing bound to it
/// is a dead exporter, and a diff shows exactly that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Absent when the address omits it, as `--web.listen-address=:9100` does.
    ///
    /// **Not normalised to a wildcard.** Go binds every interface of every family for an
    /// empty host, so writing `0.0.0.0` in its place would assert an IPv4-only bind the
    /// agent did not ask for. Absent is what the host actually said.
    pub host: Option<InetHost>,
    pub port: PortNumber,
}

impl From<&Endpoint> for Observation {
    fn from(endpoint: &Endpoint) -> Self {
        Observation::object([
            (
                "host",
                endpoint
                    .host
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("port", Observation::from(&endpoint.port)),
        ])
    }
}
