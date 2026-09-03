//! One end of a connection, as a configuration spells it.

use rastro_collector::{AbsolutePath, CollectionError, Observation};

use crate::collectors::inet::{InetHost, PortNumber};

/// How nginx spells a unix socket wherever an address is expected.
const UNIX_PREFIX: &str = "unix:";

/// An address the configuration names: what a server listens on, or where a pool sends.
///
/// **The same type for both ends on purpose.** A `listen` and an `upstream server` are the
/// same concept read from two directions, and spelling them differently would mean two
/// vocabularies for one thing in one facet. It is also the vocabulary the `sockets` facet
/// uses, which is what lets a reader hold a configured address against a bound one — those
/// two are recorded separately precisely so they can disagree.
///
/// **Both halves are optional, because nginx's are.** `listen 80;` names a port and no
/// address, `listen example.org;` names an address and no port, and each means something
/// different from the other. Filling in what nginx would default to would put rastro's
/// reckoning in the document beside the host's facts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Endpoint {
    Inet {
        host: Option<InetHost>,
        port: Option<PortNumber>,
    },
    Unix {
        path: AbsolutePath,
    },
}

impl Endpoint {
    /// Reads an address as a `listen` or an `upstream server` writes it.
    ///
    /// The IPv6 form is what makes this more than a split on the last colon: `[::]:443` is a
    /// wildcard address with a port and `[::1]` is an address with none, while `::1` on its
    /// own is not a legal listen address at all — nginx requires the brackets exactly so the
    /// colons can be told apart.
    pub fn new(value: &str) -> Result<Self, CollectionError> {
        if let Some(path) = value.strip_prefix(UNIX_PREFIX) {
            return Ok(Self::Unix {
                path: AbsolutePath::new(path, "nginx socket path")?,
            });
        }

        if let Some(rest) = value.strip_prefix('[') {
            let (host, after) = rest.split_once(']').ok_or_else(|| {
                CollectionError::new(format!(
                    "{value:?} opens an IPv6 address and never closes it"
                ))
            })?;

            return Ok(Self::Inet {
                host: Some(InetHost::new(host)?),
                port: port_of(after.strip_prefix(':'))?,
            });
        }

        match value.split_once(':') {
            Some((host, port)) => Ok(Self::Inet {
                host: Some(InetHost::new(host)?),
                port: port_of(Some(port))?,
            }),
            None => Ok(Self::bare(value)?),
        }
    }

    /// A value with no colon in it: a port on its own, or an address on its own.
    fn bare(value: &str) -> Result<Self, CollectionError> {
        match value.chars().all(|character| character.is_ascii_digit()) {
            true => Ok(Self::Inet {
                host: None,
                port: port_of(Some(value))?,
            }),
            false => Ok(Self::Inet {
                host: Some(InetHost::new(value)?),
                port: None,
            }),
        }
    }
}

fn port_of(value: Option<&str>) -> Result<Option<PortNumber>, CollectionError> {
    value
        .filter(|port| !port.is_empty())
        .map(PortNumber::parse)
        .transpose()
}

impl From<&Endpoint> for Observation {
    fn from(endpoint: &Endpoint) -> Self {
        match endpoint {
            Endpoint::Inet { host, port } => Observation::object([
                (
                    "host",
                    host.as_ref()
                        .map_or_else(Observation::null, |host| Observation::text(host.as_str())),
                ),
                (
                    "port",
                    port.as_ref().map_or_else(Observation::null, |port| {
                        Observation::integer(port.as_u16().into())
                    }),
                ),
            ]),
            Endpoint::Unix { path } => {
                Observation::object([("socket", Observation::text(path.as_str()))])
            }
        }
    }
}
