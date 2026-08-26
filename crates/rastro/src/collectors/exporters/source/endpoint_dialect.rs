//! How an agent spells the address it serves on.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::exporters::model::Endpoint;
use crate::collectors::exporters::value_objects::{SettingName, SettingValue};
use crate::collectors::inet::{InetHost, PortNumber};

/// The Prometheus convention: one flag carrying `host:port`.
const LISTEN_ADDRESS: &str = "web.listen-address";

/// cAdvisor's, which predates it: two flags.
const LISTEN_IP: &str = "listen_ip";
const PORT: &str = "port";

/// The ways the agents rastro knows are told where to listen.
///
/// Resolved into one shape here rather than left as six spellings in the document, because
/// the question an operator asks of this facet is "which port is this agent on", and
/// answering it should not require knowing which agent invented which flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointDialect {
    WebListenAddress,
    SeparateIpAndPort,
    /// The agent takes its address from a config file rather than its arguments. collectd
    /// is the case: it is started with no arguments at all, and its 9103 listener comes
    /// from the `write_prometheus` plugin in `/etc/collectd/collectd.conf`.
    NotInArguments,
}

type Settings = BTreeMap<SettingName, Option<SettingValue>>;

impl EndpointDialect {
    /// The configured endpoint, or nothing when the flags do not carry one.
    ///
    /// Nothing is an ordinary answer rather than a failure: an agent may legitimately be
    /// started without an address flag and fall back to its own compiled-in default, and
    /// writing that default here would assert a value rastro never observed.
    pub fn read(&self, settings: &Settings) -> Result<Option<Endpoint>, CollectionError> {
        match self {
            Self::WebListenAddress => match value(settings, LISTEN_ADDRESS) {
                Some(address) => Ok(Some(parse_address(address)?)),
                None => Ok(None),
            },
            Self::SeparateIpAndPort => match value(settings, PORT) {
                Some(port) => Ok(Some(Endpoint {
                    host: match value(settings, LISTEN_IP) {
                        Some(host) => Some(InetHost::new(host)?),
                        None => None,
                    },
                    port: PortNumber::parse(port)?,
                })),
                None => Ok(None),
            },
            Self::NotInArguments => Ok(None),
        }
    }
}

fn value<'a>(settings: &'a Settings, name: &str) -> Option<&'a str> {
    let name = SettingName::new(name).ok()?;

    settings.get(&name)?.as_ref().map(SettingValue::as_str)
}

/// `0.0.0.0:9100`, `:9100`, `[::]:9100`.
///
/// Split from the right, because an IPv6 address is full of colons and only the last one
/// separates the port. The brackets around such an address are punctuation for that split
/// rather than part of the address, so they come off — which is also how the sockets facet
/// records it, and these two facets exist to be compared.
fn parse_address(address: &str) -> Result<Endpoint, CollectionError> {
    let (host, port) = address.rsplit_once(':').ok_or_else(|| {
        CollectionError::new(format!(
            "{address:?} is not a listen address, because it carries no port"
        ))
    })?;

    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);

    Ok(Endpoint {
        // An empty host is `--web.listen-address=:9100`, which binds every interface of
        // every family. Absent rather than widened to `0.0.0.0`, which would assert an
        // IPv4-only bind the agent did not ask for.
        host: match host.is_empty() {
            true => None,
            false => Some(InetHost::new(host)?),
        },
        port: PortNumber::parse(port)?,
    })
}
